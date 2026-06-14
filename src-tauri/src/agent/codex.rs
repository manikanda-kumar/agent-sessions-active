use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct CodexDetector;

impl AgentDetector for CodexDetector {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_agent_processes(system, &["codex"])
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        get_codex_sessions(processes)
    }
}

#[derive(Debug, Deserialize)]
struct CodexLine {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    line_type: String,
    payload: Value,
}

#[derive(Debug)]
struct CodexMeta {
    id: String,
    cwd: String,
    timestamp: String,
    git_branch: Option<String>,
    github_url: Option<String>,
}

fn get_codex_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    if processes.is_empty() {
        return Vec::new();
    }

    let Some(codex_dir) = dirs::home_dir().map(|home| home.join(".codex")) else {
        return Vec::new();
    };
    let sessions_dir = codex_dir.join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    // Codex is now an Electron app (`codex app-server`) whose processes run with
    // cwd `/`, which is not a real workspace — matching `/` surfaces ancient
    // sessions that happened to launch from `/`. So we ignore root-cwd sessions
    // entirely. A terminal TUI process keeps its own concrete cwd and matches
    // sessions directly. For the app-server (cwd `/`), there is no per-process
    // workspace, so we attribute the single most-recently-active codex session
    // to it (gated by recency, so an idle app shows nothing stale).
    let mut cwd_to_process: HashMap<String, &AgentProcess> = HashMap::new();
    let mut app_process: Option<&AgentProcess> = None;
    for process in processes {
        match process.cwd.as_ref().map(|cwd| cwd.to_string_lossy().to_string()) {
            Some(cwd) if cwd != "/" => {
                cwd_to_process.entry(cwd).or_insert(process);
            }
            // cwd `/` or missing -> app-server; remember one as the carrier.
            _ => app_process = app_process.or(Some(process)),
        }
    }

    let mut latest_by_cwd: HashMap<String, (PathBuf, std::time::SystemTime)> = HashMap::new();
    // Most-recently-modified non-root session overall, for the app-server.
    let mut newest_overall: Option<(PathBuf, std::time::SystemTime, String)> = None;
    collect_jsonl_files(&sessions_dir, &mut |path, modified| {
        let Some(meta) = read_codex_meta(path) else {
            return;
        };
        if meta.cwd == "/" {
            return;
        }
        if cwd_to_process.contains_key(&meta.cwd) {
            let replace = latest_by_cwd
                .get(&meta.cwd)
                .map(|(_, existing_modified)| modified > *existing_modified)
                .unwrap_or(true);
            if replace {
                latest_by_cwd.insert(meta.cwd.clone(), (path.to_path_buf(), modified));
            }
        }
        if app_process.is_some() {
            let replace = newest_overall
                .as_ref()
                .map(|(_, existing, _)| modified > *existing)
                .unwrap_or(true);
            if replace {
                newest_overall = Some((path.to_path_buf(), modified, meta.cwd));
            }
        }
    });

    let mut sessions: Vec<Session> = latest_by_cwd
        .iter()
        .filter_map(|(cwd, (path, _))| {
            let process = cwd_to_process.get(cwd)?;
            parse_codex_session(path, process)
        })
        .collect();

    // Attribute the latest codex session to the desktop app, if recent enough
    // and not already shown via a concrete-cwd (TUI) process.
    if let (Some(app), Some((path, modified, cwd))) = (app_process, newest_overall) {
        let recent = std::time::SystemTime::now()
            .duration_since(modified)
            .map(|age| age < APP_SESSION_RECENCY)
            .unwrap_or(false);
        let already_shown = latest_by_cwd.contains_key(&cwd);
        if recent && !already_shown {
            if let Some(session) = parse_codex_session(&path, app) {
                sessions.push(session);
            }
        }
    }

    sessions
}

/// How recently the latest codex session must have been touched for the desktop
/// app (cwd `/`) to surface it. Beyond this the app is treated as idle.
const APP_SESSION_RECENCY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

fn collect_jsonl_files(dir: &Path, visit: &mut impl FnMut(&Path, std::time::SystemTime)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, visit);
        } else if path.extension().map(|ext| ext == "jsonl").unwrap_or(false) {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                visit(&path, modified);
            }
        }
    }
}

fn read_codex_meta(path: &Path) -> Option<CodexMeta> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(20).flatten() {
        let parsed: CodexLine = serde_json::from_str(&line).ok()?;
        if parsed.line_type != "session_meta" {
            continue;
        }
        let payload = parsed.payload;
        let id = payload.get("id")?.as_str()?.to_string();
        let cwd = payload.get("cwd")?.as_str()?.to_string();
        let timestamp = payload
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or(parsed.timestamp.as_deref())
            .unwrap_or("Unknown")
            .to_string();
        let git = payload.get("git").unwrap_or(&Value::Null);
        return Some(CodexMeta {
            id,
            cwd,
            timestamp,
            git_branch: git.get("branch").and_then(|v| v.as_str()).map(String::from),
            github_url: git
                .get("repository_url")
                .and_then(|v| v.as_str())
                .map(String::from),
        });
    }
    None
}

fn parse_codex_session(path: &Path, process: &AgentProcess) -> Option<Session> {
    let meta = read_codex_meta(path)?;
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut last_role = None;
    let mut last_message = None;
    let mut last_timestamp = Some(meta.timestamp.clone());

    for line in reader.lines().flatten() {
        let Ok(parsed) = serde_json::from_str::<CodexLine>(&line) else {
            continue;
        };
        if let Some(timestamp) = parsed.timestamp {
            last_timestamp = Some(timestamp);
        }
        if parsed.line_type != "response_item" && parsed.line_type != "event_msg" {
            continue;
        }

        if let Some((role, text)) = extract_codex_message(&parsed.payload) {
            last_role = Some(role);
            if let Some(text) = text {
                last_message = Some(text);
            }
        }
    }

    let status = match last_role.as_deref() {
        Some("assistant") => SessionStatus::Waiting,
        Some("user") => SessionStatus::Processing,
        _ if process.cpu_usage > 5.0 => SessionStatus::Processing,
        _ => SessionStatus::Idle,
    };

    let project_name = meta
        .cwd
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    Some(Session {
        id: meta.id,
        agent_type: AgentType::Codex,
        project_name,
        project_path: meta.cwd,
        git_branch: meta.git_branch,
        github_url: meta.github_url,
        status,
        last_message: last_message.map(truncate),
        last_message_role: last_role,
        last_activity_at: last_timestamp.unwrap_or_else(|| "Unknown".to_string()),
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    })
}

fn extract_codex_message(payload: &Value) -> Option<(String, Option<String>)> {
    if let Some(role) = payload.get("role").and_then(|v| v.as_str()) {
        return Some((role.to_string(), extract_text(payload.get("content")?)));
    }

    if let Some(message) = payload.get("message") {
        let role = message.get("role")?.as_str()?.to_string();
        return Some((role, extract_text(message.get("content")?)));
    }

    None
}

fn extract_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(|item| {
            item.get("text")
                .or_else(|| item.get("input_text"))
                .and_then(|v| v.as_str())
                .filter(|text| !text.is_empty())
                .map(String::from)
        }),
        _ => None,
    }
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
    fn extracts_user_message_from_response_item() {
        let payload: Value = serde_json::from_str(
            r#"{"role":"user","content":[{"type":"input_text","text":"hello there"}]}"#,
        )
        .unwrap();
        let (role, text) = extract_codex_message(&payload).unwrap();
        assert_eq!(role, "user");
        assert_eq!(text.as_deref(), Some("hello there"));
    }

    #[test]
    fn app_session_recency_is_one_day() {
        assert_eq!(APP_SESSION_RECENCY.as_secs(), 86_400);
    }
}
