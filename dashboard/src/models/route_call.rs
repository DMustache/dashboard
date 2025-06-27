use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteCall {
    pub path: String,
    pub method: String,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_delta_bytes: Option<isize>,
}
