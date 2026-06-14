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

    let Some(sessions_dir) = dirs::home_dir().map(|home| home.join(".codex").join("sessions"))
    else {
        return Vec::new();
    };
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let cwd_to_process: HashMap<String, &AgentProcess> = processes
        .iter()
        .filter_map(|process| {
            process
                .cwd
                .as_ref()
                .map(|cwd| (cwd.to_string_lossy().to_string(), process))
        })
        .collect();

    let mut latest_by_cwd: HashMap<String, (PathBuf, std::time::SystemTime)> = HashMap::new();
    collect_jsonl_files(&sessions_dir, &mut |path, modified| {
        if let Some(meta) = read_codex_meta(path) {
            if cwd_to_process.contains_key(&meta.cwd) {
                let replace = latest_by_cwd
                    .get(&meta.cwd)
                    .map(|(_, existing_modified)| modified > *existing_modified)
                    .unwrap_or(true);
                if replace {
                    latest_by_cwd.insert(meta.cwd, (path.to_path_buf(), modified));
                }
            }
        }
    });

    latest_by_cwd
        .into_iter()
        .filter_map(|(cwd, (path, _))| {
            let process = cwd_to_process.get(&cwd)?;
            parse_codex_session(&path, process)
        })
        .collect()
}

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
