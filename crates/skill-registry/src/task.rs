use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct JobPreconditions {
    pub required_memory_bytes: u64,
    pub required_resources: Vec<String>,
    pub required_skills: Vec<String>,
    pub timeout_ticks: u64,
    pub max_retries: u8,
}

impl Default for JobPreconditions {
    fn default() -> Self {
        JobPreconditions {
            required_memory_bytes: 0,
            required_resources: Vec::new(),
            required_skills: Vec::new(),
            timeout_ticks: 1000,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed(Vec<u8>),
    Failed(&'static str),
    TimedOut,
}

#[derive(Debug, Clone)]
pub struct TaskSchema {
    pub name: String,
    pub description: String,
    pub input: Vec<u8>,
    pub preconditions: JobPreconditions,
    pub status: TaskStatus,
    pub started_at_tick: u64,
    pub attempts: u8,
}
