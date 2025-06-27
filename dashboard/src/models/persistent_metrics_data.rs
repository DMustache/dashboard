use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    globals,
    models::{MemoryAllocation, MemoryStats, RouteCall, RouteStats, SystemSnapshot},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentMetricsData {
    pub system_snapshots: VecDeque<SystemSnapshot>,
    pub route_calls: VecDeque<RouteCall>,
    pub route_stats: HashMap<String, RouteStats>,
    pub memory_events: VecDeque<MemoryAllocation>,
    pub memory_stats: MemoryStats,
    pub blacklist: HashSet<String>,
}

impl PersistentMetricsData {
    pub fn collect() -> Self {
        Self {
            system_snapshots: globals::system_snapshots().lock().unwrap().clone(),
            route_calls: globals::route_calls().lock().unwrap().clone(),
            route_stats: globals::route_stats().read().unwrap().clone(),
            memory_events: globals::memory_events().lock().unwrap().clone(),
            memory_stats: globals::memory_stats().read().unwrap().clone(),
            blacklist: globals::blacklist().read().unwrap().clone(),
        }
    }
}
