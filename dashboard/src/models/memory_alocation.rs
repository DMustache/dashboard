use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryAllocation {
    pub size: usize,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    pub allocation_type: String,
}
