use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryStats {
    pub total_allocations: usize,
    pub total_deallocations: usize,
    pub current_memory: usize,
    pub peak_memory: usize,
    pub allocation_sizes: HashMap<String, usize>,
}
