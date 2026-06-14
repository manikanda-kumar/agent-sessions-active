use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::SystemTime;

pub struct AgyDetector;

impl AgentDetector for AgyDetector {
    fn name(&self) -> &'static str {
        "Agy"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Agy
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_agent_processes(system, &["agy", "antigravity"])
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        get_agy_sessions(processes)
    }
}

#[derive(Debug, Deserialize)]
struct AgyHistoryEntry {
    display: String,
    timestamp: u64,
    workspace: String,
    #[serde(rename = "conversationId")]
    conversation_id: Option<String>,
}

fn get_agy_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    if processes.is_empty() {
        return Vec::new();
    }

    let history_path = dirs::home_dir()
        .map(|home| home.join(".gemini").join("antigravity-cli").join("history.jsonl"));

    let Some(path) = history_path else {
        return Vec::new();
    };
    if !path.exists() {
        return Vec::new();
    }

    let entries = read_history_entries(&path);
    if entries.is_empty() {
        return Vec::new();
    }

    // Build a map of workspace -> most recent entry
    let mut workspace_to_entry: HashMap<String, &AgyHistoryEntry> = HashMap::new();
    for entry in &entries {
        let existing = workspace_to_entry.get(&entry.workspace);
        if existing.is_none() || existing.unwrap().timestamp < entry.timestamp {
            workspace_to_entry.insert(entry.workspace.clone(), entry);
        }
    }

    let mut used_ids: HashSet<String> = HashSet::new();
    let mut sessions = Vec::new();

    for process in processes {
        let Some(cwd) = &process.cwd else {
            continue;
        };
        let cwd_str = cwd.to_string_lossy().to_string();

        // Find the most recent entry for this workspace
        let Some(entry) = workspace_to_entry.get(&cwd_str) else {
            continue;
        };

        let session_id = entry
            .conversation_id
            .clone()
            .unwrap_or_else(|| format!("agy-{}", entry.timestamp));

        if used_ids.contains(&session_id) {
            continue;
        }
        used_ids.insert(session_id.clone());

        let project_name = cwd_str
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .unwrap_or("Unknown")
            .to_string();

        let last_activity_at = millis_to_iso(entry.timestamp)
            .unwrap_or_else(|| "Unknown".to_string());

        // Determine status based on recency and CPU usage
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let age_ms = now.saturating_sub(entry.timestamp);
        let is_recent = age_ms < 30_000; // 30 seconds

        let status = if process.cpu_usage > 5.0 || is_recent {
            SessionStatus::Processing
        } else {
            SessionStatus::Waiting
        };

        sessions.push(Session {
            id: session_id,
            agent_type: AgentType::Agy,
            project_name,
            project_path: cwd_str,
            git_branch: None,
            github_url: None,
            status,
            last_message: Some(entry.display.clone()),
            last_message_role: Some("user".to_string()),
            last_activity_at,
            pid: process.pid,
            cpu_usage: process.cpu_usage,
            active_subagent_count: 0,
        });
    }

    sessions
}

/// Read history.jsonl entries from the end (most recent first).
/// Returns entries in reverse chronological order.
fn read_history_entries(path: &Path) -> Vec<AgyHistoryEntry> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<AgyHistoryEntry>(&line) {
            entries.push(entry);
        }
    }

    // Sort by timestamp descending (most recent first)
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    entries
}

fn millis_to_iso(timestamp: u64) -> Option<String> {
    let seconds = timestamp / 1000;
    let nanos = ((timestamp % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(seconds as i64, nanos).map(|dt| dt.to_rfc3339())
}
