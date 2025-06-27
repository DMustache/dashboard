use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemSnapshot {
    pub timestamp: u64,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub process_cpu_usage: f32,
    pub process_memory_usage: u64,
    pub allocations: usize,
    pub deallocations: usize,
    pub current_memory: usize,
    pub peak_memory: usize,
}
