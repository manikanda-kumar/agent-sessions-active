use super::AgentProcess;
use std::collections::HashSet;

fn command_basename(value: &std::ffi::OsStr) -> String {
    std::path::Path::new(value)
        .file_name()
        .unwrap_or(value)
        .to_string_lossy()
        .to_lowercase()
}

fn first_arg_matches(process: &sysinfo::Process, names: &[&str]) -> bool {
    let Some(first_arg) = process.cmd().first() else {
        return false;
    };
    let basename = command_basename(first_arg);
    names.iter().any(|name| basename == *name)
}

fn is_our_app(process_name: &str) -> bool {
    process_name.contains("claude-sessions")
        || process_name.contains("tauri-temp")
        || process_name.contains("agent-sessions")
}

/// Find top-level CLI processes by executable name.
pub fn find_agent_processes(system: &sysinfo::System, names: &[&str]) -> Vec<AgentProcess> {
    let mut matching_pids = HashSet::new();
    for (pid, process) in system.processes() {
        if first_arg_matches(process, names) {
            matching_pids.insert(*pid);
        }
    }

    let mut processes = Vec::new();
    for (pid, process) in system.processes() {
        let process_name = process.name().to_string_lossy().to_lowercase();
        if !first_arg_matches(process, names) || is_our_app(&process_name) {
            continue;
        }

        if let Some(parent_pid) = process.parent() {
            if matching_pids.contains(&parent_pid) {
                continue;
            }
        }

        processes.push(AgentProcess {
            pid: pid.as_u32(),
            cpu_usage: process.cpu_usage(),
            cwd: process.cwd().map(|p| p.to_path_buf()),
        });
    }

    processes
}
