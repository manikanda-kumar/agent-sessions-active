use super::{process::find_agent_processes, AgentDetector, AgentProcess};
use crate::session::parser::get_sessions_in_dir;
use crate::session::{AgentType, Session};
use std::path::PathBuf;

pub struct JsonlAgentDetector {
    pub display_name: &'static str,
    pub agent_type: AgentType,
    pub process_names: &'static [&'static str],
    pub projects_dir: fn() -> Option<PathBuf>,
}

impl AgentDetector for JsonlAgentDetector {
    fn name(&self) -> &'static str {
        self.display_name
    }

    fn agent_type(&self) -> AgentType {
        self.agent_type.clone()
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_agent_processes(system, self.process_names)
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        let Some(projects_dir) = (self.projects_dir)() else {
            return Vec::new();
        };
        get_sessions_in_dir(
            processes,
            self.agent_type.clone(),
            projects_dir,
            self.display_name,
        )
    }
}

pub fn pi_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".pi").join("agent").join("sessions"))
}

pub fn droid_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".factory").join("sessions"))
}
