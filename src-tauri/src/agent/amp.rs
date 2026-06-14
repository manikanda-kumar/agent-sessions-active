use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub struct AmpDetector;

impl AgentDetector for AmpDetector {
    fn name(&self) -> &'static str {
        "Amp"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Amp
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_agent_processes(system, &["amp"])
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        get_amp_sessions(processes)
    }
}

// ─── Plugin status file (authoritative) ────────────────────────────────────
// The `agent-sessions-status` Amp plugin runs inside each amp process and
// writes ~/.local/share/amp/agent-sessions/<pid>.json with the exact thread id,
// cwd, and live turn status. Joined to a process by pid, this needs no
// guessing. When the plugin isn't installed we fall back to the session.json /
// history.jsonl heuristic below.

#[derive(Debug, Deserialize)]
struct AmpPluginStatus {
    #[serde(rename = "threadId")]
    thread_id: Option<String>,
    status: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<u64>,
}

// ─── session.json ──────────────────────────────────────────────────────────
// Amp stores threads server-side now; the local `threads/*.json` files are
// stale snapshots and no longer track the active session. The authoritative
// local signal for "which thread is this process on, and when was it last
// touched" lives in `session.json` (`lastThreadId` / `lastThreadByTerminal`).

#[derive(Debug, Deserialize, Default)]
struct AmpSessionState {
    #[serde(rename = "lastThreadId")]
    last_thread_id: Option<String>,
    #[serde(rename = "lastThreadByTerminal", default)]
    last_thread_by_terminal: HashMap<String, AmpTerminalThread>,
}

#[derive(Debug, Deserialize)]
struct AmpTerminalThread {
    #[serde(rename = "updatedAt")]
    updated_at: u64,
    #[serde(rename = "lastThreadId")]
    last_thread_id: String,
}

/// One active thread: id + last-touched time, sorted most-recent-first.
struct RecentThread {
    thread_id: String,
    updated_at: u64,
}

// ─── history.jsonl ─────────────────────────────────────────────────────────
// Append-only log of submitted prompts: `{ "text": ..., "cwd": ... }`, one per
// line, oldest first. `cwd` is the only local join key back to a process.

#[derive(Debug, Deserialize)]
struct AmpHistoryEntry {
    text: Option<String>,
    cwd: Option<String>,
}

// ─── Optional local thread file (back-compat / enrichment) ──────────────────

#[derive(Debug, Deserialize)]
struct AmpThread {
    #[allow(dead_code)]
    id: String,
    title: Option<String>,
    #[serde(default)]
    messages: Vec<AmpMessage>,
}

#[derive(Debug, Deserialize)]
struct AmpMessage {
    role: String,
    content: Value,
}

fn get_amp_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    if processes.is_empty() {
        return Vec::new();
    }

    let Some(amp_dir) = dirs::home_dir()
        .map(|home| home.join(".local").join("share").join("amp"))
    else {
        return Vec::new();
    };

    let state = read_session_state(&amp_dir);
    let history = read_history(&amp_dir);

    // 1. Authoritative: per-pid status written by the amp plugin.
    let plugin_by_pid: HashMap<u32, AmpPluginStatus> = processes
        .iter()
        .filter_map(|process| {
            read_plugin_status(&amp_dir, process.pid).map(|status| (process.pid, status))
        })
        .collect();

    // Threads already pinned to a process by the plugin must not be reused by
    // the recency heuristic for the remaining processes.
    let claimed: HashSet<String> = plugin_by_pid
        .values()
        .filter_map(|status| status.thread_id.clone())
        .collect();

    // 2. Fallback: pair the processes with no plugin data against the most
    // recent unclaimed threads from session.json. Rank each process by how
    // recently its cwd appears in history.jsonl; cwd is the join key to the
    // process, thread updatedAt to the (cloud) thread. Recency aligns the two.
    let fallback_threads: Vec<RecentThread> = recent_threads(&state)
        .into_iter()
        .filter(|thread| !claimed.contains(&thread.thread_id))
        .collect();

    let mut ranked: Vec<&AgentProcess> = processes
        .iter()
        .filter(|process| !plugin_by_pid.contains_key(&process.pid))
        .collect();
    ranked.sort_by(|a, b| history_rank(&history, b).cmp(&history_rank(&history, a)));

    let fallback_by_pid: HashMap<u32, &RecentThread> = ranked
        .iter()
        .enumerate()
        .filter_map(|(index, process)| fallback_threads.get(index).map(|thread| (process.pid, thread)))
        .collect();

    processes
        .iter()
        .map(|process| {
            let plugin = plugin_by_pid.get(&process.pid);
            let recent = fallback_by_pid.get(&process.pid).copied();
            build_session(&amp_dir, process, plugin, recent, &history)
        })
        .collect()
}

fn read_plugin_status(amp_dir: &Path, pid: u32) -> Option<AmpPluginStatus> {
    let path = amp_dir
        .join("agent-sessions")
        .join(format!("{}.json", pid));
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_session_state(amp_dir: &Path) -> AmpSessionState {
    fs::read_to_string(amp_dir.join("session.json"))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Active threads sorted most-recently-updated first, deduplicated by id.
fn recent_threads(state: &AmpSessionState) -> Vec<RecentThread> {
    let mut threads: Vec<RecentThread> = state
        .last_thread_by_terminal
        .values()
        .map(|terminal| RecentThread {
            thread_id: terminal.last_thread_id.clone(),
            updated_at: terminal.updated_at,
        })
        .collect();
    threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    threads.dedup_by(|a, b| a.thread_id == b.thread_id);

    // Guarantee the single most-recent thread is represented even if the
    // per-terminal map is empty (older Amp builds).
    if let Some(last_id) = &state.last_thread_id {
        if !threads.iter().any(|thread| &thread.thread_id == last_id) {
            threads.insert(
                0,
                RecentThread {
                    thread_id: last_id.clone(),
                    updated_at: 0,
                },
            );
        }
    }
    threads
}

fn read_history(amp_dir: &Path) -> Vec<AmpHistoryEntry> {
    let Ok(content) = fs::read_to_string(amp_dir.join("history.jsonl")) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str::<AmpHistoryEntry>(line).ok())
        .collect()
}

/// Highest line index in history whose cwd matches the process (0 = no match).
fn history_rank(history: &[AmpHistoryEntry], process: &AgentProcess) -> usize {
    let Some(cwd) = process_cwd(process) else {
        return 0;
    };
    history
        .iter()
        .rposition(|entry| entry.cwd.as_deref() == Some(cwd.as_str()))
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

/// Last prompt text submitted from the process cwd.
fn last_prompt_for_cwd(history: &[AmpHistoryEntry], cwd: &str) -> Option<String> {
    history
        .iter()
        .rev()
        .find(|entry| entry.cwd.as_deref() == Some(cwd))
        .and_then(|entry| entry.text.clone())
        .filter(|text| !text.is_empty())
}

fn build_session(
    amp_dir: &Path,
    process: &AgentProcess,
    plugin: Option<&AmpPluginStatus>,
    recent: Option<&RecentThread>,
    history: &[AmpHistoryEntry],
) -> Session {
    let cwd = process_cwd(process).unwrap_or_default();
    let fallback_name = cwd
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    // Thread id: plugin is authoritative; else the recency-matched thread.
    let thread_id = plugin
        .and_then(|plugin| plugin.thread_id.clone())
        .or_else(|| recent.map(|recent| recent.thread_id.clone()));

    // Enrich from a local thread file when it still exists (older Amp builds
    // that store threads on disk). Cloud-only threads have no local file.
    let thread = thread_id
        .as_ref()
        .map(|id| amp_dir.join("threads").join(format!("{}.json", id)))
        .filter(|path| path.exists())
        .and_then(|path| read_thread(&path));

    let last_role = thread.as_ref().and_then(|thread| {
        thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user" || message.role == "assistant")
            .map(|message| message.role.clone())
    });

    let last_message = thread
        .as_ref()
        .and_then(last_thread_text)
        .or_else(|| last_prompt_for_cwd(history, &cwd))
        .map(truncate);

    let project_name = thread
        .as_ref()
        .and_then(|thread| thread.title.clone())
        .filter(|title| !title.is_empty())
        .unwrap_or(fallback_name);

    // Timestamp: plugin updatedAt, else session.json updatedAt, else now.
    let last_activity_at = plugin
        .and_then(|plugin| plugin.updated_at)
        .filter(|ts| *ts > 0)
        .or_else(|| recent.map(|recent| recent.updated_at).filter(|ts| *ts > 0))
        .and_then(millis_to_iso)
        .or_else(|| millis_to_iso(now_millis()))
        .unwrap_or_else(|| "Unknown".to_string());

    // Status: plugin's live turn status wins; else infer from messages/cpu.
    let status = plugin
        .and_then(|plugin| plugin.status.as_deref())
        .map(plugin_status_to_session_status)
        .unwrap_or_else(|| match last_role.as_deref() {
            Some("assistant") => SessionStatus::Waiting,
            Some("user") => SessionStatus::Processing,
            _ if process.cpu_usage > 5.0 => SessionStatus::Processing,
            _ => SessionStatus::Idle,
        });

    Session {
        id: thread_id.unwrap_or_else(|| format!("amp-{}", process.pid)),
        agent_type: AgentType::Amp,
        project_name,
        project_path: cwd,
        git_branch: None,
        github_url: None,
        status,
        last_message,
        last_message_role: last_role,
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    }
}

fn plugin_status_to_session_status(status: &str) -> SessionStatus {
    match status {
        "thinking" => SessionStatus::Thinking,
        "processing" => SessionStatus::Processing,
        "waiting" => SessionStatus::Waiting,
        _ => SessionStatus::Idle,
    }
}

fn process_cwd(process: &AgentProcess) -> Option<String> {
    process
        .cwd
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
}

fn read_thread(path: &Path) -> Option<AmpThread> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn last_thread_text(thread: &AmpThread) -> Option<String> {
    thread
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user" || message.role == "assistant")
        .and_then(|message| extract_amp_text(&message.content))
}

fn extract_amp_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(|item| {
            item.get("text")
                .and_then(|value| value.as_str())
                .filter(|text| !text.is_empty())
                .map(String::from)
        }),
        _ => None,
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn millis_to_iso(timestamp: u64) -> Option<String> {
    let seconds = timestamp / 1000;
    let nanos = ((timestamp % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(seconds as i64, nanos).map(|dt| dt.to_rfc3339())
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
    fn recent_threads_sorted_by_updated_at_desc() {
        let state: AmpSessionState = serde_json::from_str(
            r#"{
                "lastThreadId": "T-new",
                "lastThreadByTerminal": {
                    "term-a": { "updatedAt": 100, "lastThreadId": "T-old" },
                    "term-b": { "updatedAt": 300, "lastThreadId": "T-new" },
                    "term-c": { "updatedAt": 200, "lastThreadId": "T-mid" }
                }
            }"#,
        )
        .unwrap();

        let threads = recent_threads(&state);
        let ids: Vec<&str> = threads.iter().map(|t| t.thread_id.as_str()).collect();
        assert_eq!(ids, vec!["T-new", "T-mid", "T-old"]);
        assert_eq!(threads[0].updated_at, 300);
    }

    #[test]
    fn last_thread_id_included_when_terminal_map_empty() {
        let state: AmpSessionState = serde_json::from_str(
            r#"{ "lastThreadId": "T-solo", "lastThreadByTerminal": {} }"#,
        )
        .unwrap();

        let threads = recent_threads(&state);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].thread_id, "T-solo");
    }

    #[test]
    fn last_prompt_matches_by_cwd() {
        let history = vec![
            AmpHistoryEntry {
                text: Some("first".into()),
                cwd: Some("/a".into()),
            },
            AmpHistoryEntry {
                text: Some("second".into()),
                cwd: Some("/b".into()),
            },
            AmpHistoryEntry {
                text: Some("third".into()),
                cwd: Some("/a".into()),
            },
        ];
        assert_eq!(last_prompt_for_cwd(&history, "/a").as_deref(), Some("third"));
        assert_eq!(last_prompt_for_cwd(&history, "/b").as_deref(), Some("second"));
        assert_eq!(last_prompt_for_cwd(&history, "/c"), None);
    }
}
