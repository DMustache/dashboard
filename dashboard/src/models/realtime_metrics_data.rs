use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::{MemoryAllocation, MemoryStats, RouteCall, RouteStats, SystemSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeMetricsData {
    pub system_snapshot: SystemSnapshot,
    pub recent_route_calls: Vec<RouteCall>,
    pub route_stats: HashMap<String, RouteStats>,
    pub recent_memory_events: Vec<MemoryAllocation>,
    pub memory_stats: MemoryStats,
    pub blacklist: Vec<String>,
}
