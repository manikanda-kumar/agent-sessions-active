use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

#[derive(Debug, Deserialize)]
struct AmpThread {
    id: String,
    created: Option<u64>,
    title: Option<String>,
    messages: Vec<AmpMessage>,
    env: Option<AmpEnvironment>,
}

#[derive(Debug, Deserialize)]
struct AmpEnvironment {
    initial: Option<AmpInitialEnvironment>,
}

#[derive(Debug, Deserialize)]
struct AmpInitialEnvironment {
    trees: Option<Vec<AmpWorkspaceTree>>,
}

#[derive(Debug, Deserialize)]
struct AmpWorkspaceTree {
    uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmpMessage {
    role: String,
    content: Value,
    meta: Option<AmpMeta>,
}

#[derive(Debug, Deserialize)]
struct AmpMeta {
    #[serde(rename = "sentAt")]
    sent_at: Option<u64>,
}

fn get_amp_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    if processes.is_empty() {
        return Vec::new();
    }

    let Some(threads_dir) = dirs::home_dir().map(|home| {
        home.join(".local")
            .join("share")
            .join("amp")
            .join("threads")
    }) else {
        return Vec::new();
    };
    if !threads_dir.exists() {
        return Vec::new();
    }

    let candidates = collect_thread_candidates(&threads_dir);
    let mut used_threads: HashSet<String> = HashSet::new();
    processes
        .iter()
        .filter_map(|process| {
            let thread_path = matching_thread(&candidates, process, &used_threads)
                .or_else(|| latest_unused_thread(&candidates, &used_threads))?;
            let thread_id = thread_path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            used_threads.insert(thread_id);
            parse_amp_session(&thread_path, process)
        })
        .collect()
}

struct AmpThreadCandidate {
    path: PathBuf,
    modified: SystemTime,
    workspace_paths: Vec<String>,
}

fn collect_thread_candidates(threads_dir: &Path) -> Vec<AmpThreadCandidate> {
    let mut candidates = Vec::new();
    let Ok(entries) = fs::read_dir(threads_dir) else {
        return candidates;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map(|ext| ext == "json").unwrap_or(false) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let workspace_paths = read_amp_workspace_paths(&path);
        candidates.push(AmpThreadCandidate {
            path,
            modified,
            workspace_paths,
        });
    }
    candidates.sort_by(|a, b| b.modified.cmp(&a.modified));
    candidates
}

fn matching_thread(
    candidates: &[AmpThreadCandidate],
    process: &AgentProcess,
    used_threads: &HashSet<String>,
) -> Option<PathBuf> {
    let cwd = process.cwd.as_ref()?.to_string_lossy().to_string();
    candidates
        .iter()
        .find(|candidate| {
            !is_used_thread(&candidate.path, used_threads)
                && candidate.workspace_paths.iter().any(|path| path == &cwd)
        })
        .map(|candidate| candidate.path.clone())
}

fn latest_unused_thread(
    candidates: &[AmpThreadCandidate],
    used_threads: &HashSet<String>,
) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| !is_used_thread(&candidate.path, used_threads))
        .map(|candidate| candidate.path.clone())
}

fn is_used_thread(path: &Path, used_threads: &HashSet<String>) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|stem| used_threads.contains(stem))
        .unwrap_or(false)
}

fn read_amp_workspace_paths(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(thread) = serde_json::from_str::<AmpThread>(&content) else {
        return Vec::new();
    };
    amp_workspace_paths(&thread)
}

fn amp_workspace_paths(thread: &AmpThread) -> Vec<String> {
    thread
        .env
        .as_ref()
        .and_then(|env| env.initial.as_ref())
        .and_then(|initial| initial.trees.as_ref())
        .map(|trees| {
            trees
                .iter()
                .filter_map(|tree| tree.uri.as_deref())
                .filter_map(file_uri_to_path)
                .collect()
        })
        .unwrap_or_default()
}

fn file_uri_to_path(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://")?;
    percent_decode(path)
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn parse_amp_session(path: &Path, process: &AgentProcess) -> Option<Session> {
    let content = fs::read_to_string(path).ok()?;
    let thread: AmpThread = serde_json::from_str(&content).ok()?;
    let cwd = process.cwd.as_ref()?.to_string_lossy().to_string();
    let project_name = cwd
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    let last_message = thread
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user" || message.role == "assistant");
    let last_role = last_message.map(|message| message.role.clone());
    let last_text = last_message.and_then(|message| extract_amp_text(&message.content));
    let last_activity_at = last_message
        .and_then(|message| message.meta.as_ref().and_then(|meta| meta.sent_at))
        .or(thread.created)
        .and_then(millis_to_iso)
        .unwrap_or_else(|| "Unknown".to_string());

    let status = match last_role.as_deref() {
        Some("assistant") => SessionStatus::Waiting,
        Some("user") => SessionStatus::Processing,
        _ if process.cpu_usage > 5.0 => SessionStatus::Processing,
        _ => SessionStatus::Idle,
    };

    Some(Session {
        id: thread.id,
        agent_type: AgentType::Amp,
        project_name: thread.title.unwrap_or(project_name),
        project_path: cwd,
        git_branch: None,
        github_url: None,
        status,
        last_message: last_text.map(truncate),
        last_message_role: last_role,
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    })
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
    fn extracts_amp_workspace_paths_from_env_tree_uris() {
        let thread: AmpThread = serde_json::from_str(
            r#"{
                "id": "T-test",
                "created": 1710000000000,
                "title": "Example",
                "messages": [],
                "env": {
                    "initial": {
                        "trees": [
                            {
                                "displayName": "repo",
                                "uri": "file:///Users/test/Github/repo%20with%20spaces"
                            }
                        ]
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            amp_workspace_paths(&thread),
            vec!["/Users/test/Github/repo with spaces".to_string()]
        );
    }
}
