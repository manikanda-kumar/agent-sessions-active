use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct GrokDetector;

impl AgentDetector for GrokDetector {
    fn name(&self) -> &'static str {
        "Grok"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Grok
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_agent_processes(system, &["grok"])
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        get_grok_sessions(processes)
    }
}

// `~/.grok/active_sessions.json` is the authoritative live map: each running
// grok process records its pid, session id, and cwd here.
#[derive(Debug, Deserialize)]
struct GrokActiveSession {
    session_id: String,
    pid: u32,
    cwd: String,
}

// `<session>/summary.json` carries the generated title and last-active time.
#[derive(Debug, Deserialize)]
struct GrokSummary {
    generated_title: Option<String>,
    session_summary: Option<String>,
    last_active_at: Option<String>,
    updated_at: Option<String>,
}

// `<cwd>/prompt_history.jsonl` lines: the user's submitted prompts.
#[derive(Debug, Deserialize)]
struct GrokPrompt {
    session_id: Option<String>,
    prompt: Option<String>,
}

fn get_grok_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    if processes.is_empty() {
        return Vec::new();
    }

    let Some(grok_home) = dirs::home_dir().map(|home| home.join(".grok")) else {
        return Vec::new();
    };

    let active = read_active_sessions(&grok_home);
    if active.is_empty() {
        return Vec::new();
    }

    processes
        .iter()
        .filter_map(|process| {
            // Match by pid (authoritative); fall back to cwd if pid rotated.
            let entry = active
                .iter()
                .find(|a| a.pid == process.pid)
                .or_else(|| {
                    let cwd = process.cwd.as_ref()?.to_string_lossy().to_string();
                    active.iter().find(|a| a.cwd == cwd)
                })?;
            Some(build_session(&grok_home, entry, process))
        })
        .collect()
}

fn read_active_sessions(grok_home: &Path) -> Vec<GrokActiveSession> {
    fs::read_to_string(grok_home.join("active_sessions.json"))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn build_session(
    grok_home: &Path,
    entry: &GrokActiveSession,
    process: &AgentProcess,
) -> Session {
    let session_dir = find_session_dir(grok_home, &entry.cwd, &entry.session_id);
    let summary = session_dir
        .as_ref()
        .and_then(|dir| read_summary(dir));

    let project_name = entry
        .cwd
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    // last_active_at / updated_at are already RFC3339 strings.
    let last_activity_at = summary
        .as_ref()
        .and_then(|s| s.last_active_at.clone().or_else(|| s.updated_at.clone()))
        .unwrap_or_else(|| "Unknown".to_string());

    // Prefer the title for at-a-glance context; else the last prompt.
    let title = summary.as_ref().and_then(|s| {
        s.generated_title
            .clone()
            .or_else(|| s.session_summary.clone())
            .filter(|t| !t.is_empty())
    });
    let last_prompt = session_dir
        .as_ref()
        .and_then(|dir| last_prompt_for_session(dir, &entry.session_id));
    let last_message = title.or(last_prompt).map(truncate);

    // No cheap signal for the live turn state (chat_history is large), so infer
    // from CPU and recency, matching the other lightweight detectors.
    let recent = last_activity_iso_to_millis(&last_activity_at)
        .map(|ts| now_millis().saturating_sub(ts) < 30_000)
        .unwrap_or(false);
    let status = if process.cpu_usage > 5.0 || recent {
        SessionStatus::Processing
    } else {
        SessionStatus::Waiting
    };

    Session {
        id: entry.session_id.clone(),
        agent_type: AgentType::Grok,
        project_name,
        project_path: entry.cwd.clone(),
        git_branch: None,
        github_url: None,
        status,
        last_message,
        last_message_role: Some("user".to_string()),
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    }
}

/// Locate `<sessions>/<encoded-cwd>/<session_id>/`. The cwd is percent-encoded
/// in the directory name, so scan the cwd dirs rather than re-encode it.
fn find_session_dir(grok_home: &Path, _cwd: &str, session_id: &str) -> Option<PathBuf> {
    let sessions_dir = grok_home.join("sessions");
    let entries = fs::read_dir(&sessions_dir).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join(session_id);
        if candidate.join("summary.json").exists() {
            return Some(candidate);
        }
    }
    None
}

fn read_summary(session_dir: &Path) -> Option<GrokSummary> {
    let content = fs::read_to_string(session_dir.join("summary.json")).ok()?;
    serde_json::from_str(&content).ok()
}

/// Last prompt for this session from the cwd-level `prompt_history.jsonl`.
fn last_prompt_for_session(session_dir: &Path, session_id: &str) -> Option<String> {
    // prompt_history.jsonl lives in the cwd dir, one level up from the session.
    let history = session_dir.parent()?.join("prompt_history.jsonl");
    let file = fs::File::open(history).ok()?;
    let mut last: Option<String> = None;
    for line in BufReader::new(file).lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<GrokPrompt>(&line) {
            if entry.session_id.as_deref() == Some(session_id) {
                if let Some(prompt) = entry.prompt.filter(|p| !p.is_empty()) {
                    last = Some(prompt);
                }
            }
        }
    }
    last
}

fn last_activity_iso_to_millis(iso: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.timestamp_millis().max(0) as u64)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate(text: String) -> String {
    if text.chars().count() > 100 {
        format!("{}...", text.chars().take(100).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_sessions() {
        let active: Vec<GrokActiveSession> = serde_json::from_str(
            r#"[{"session_id":"019ec776","pid":51331,"cwd":"/Users/me/repo","opened_at":"2026-06-14T18:48:25Z"}]"#,
        )
        .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].pid, 51331);
        assert_eq!(active[0].cwd, "/Users/me/repo");
    }

    #[test]
    fn parses_summary_title_and_time() {
        let summary: GrokSummary = serde_json::from_str(
            r#"{"generated_title":"My Task","session_summary":"sum","last_active_at":"2026-06-14T19:08:08.169736Z","updated_at":"2026-06-14T18:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(summary.generated_title.as_deref(), Some("My Task"));
        assert_eq!(
            summary.last_active_at.as_deref(),
            Some("2026-06-14T19:08:08.169736Z")
        );
    }

    #[test]
    fn rfc3339_to_millis_roundtrips() {
        let ms = last_activity_iso_to_millis("2026-06-14T19:08:08.000Z").unwrap();
        // Sanity: a 2026 timestamp is well past 2020 in millis.
        assert!(ms > 1_700_000_000_000);
    }
}
