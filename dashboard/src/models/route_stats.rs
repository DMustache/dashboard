use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::SLIDING_WINDOW_SIZE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStats {
    pub total_calls: usize,
    pub avg_duration_ms: f64,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub success_count: usize,
    pub error_count: usize,
    pub total_memory_delta_bytes: isize,
    pub avg_memory_delta_bytes: f64,
    pub min_memory_delta_bytes: isize,
    pub max_memory_delta_bytes: isize,

    #[serde(default = "VecDeque::new")]
    pub recent_durations_ms: VecDeque<u64>,
    #[serde(default = "VecDeque::new")]
    pub recent_memory_deltas: VecDeque<isize>,
    #[serde(skip)]
    pub sliding_avg_duration_ms: f64,
    #[serde(skip)]
    pub sliding_avg_memory_delta_bytes: f64,
}

impl Default for RouteStats {
    fn default() -> Self {
        Self {
            total_calls: 0,
            avg_duration_ms: 0.0,
            min_duration_ms: u64::MAX,
            max_duration_ms: 0,
            success_count: 0,
            error_count: 0,
            total_memory_delta_bytes: 0,
            avg_memory_delta_bytes: 0.0,
            min_memory_delta_bytes: isize::MAX,
            max_memory_delta_bytes: isize::MIN,
            recent_durations_ms: VecDeque::with_capacity(SLIDING_WINDOW_SIZE),
            recent_memory_deltas: VecDeque::with_capacity(SLIDING_WINDOW_SIZE),
            sliding_avg_duration_ms: 0.0,
            sliding_avg_memory_delta_bytes: 0.0,
        }
    }
}

impl RouteStats {
    pub fn update_sliding_averages(&mut self) {
        if self.recent_durations_ms.is_empty() {
            self.sliding_avg_duration_ms = 0.0;
        } else {
            let sum: u64 = self.recent_durations_ms.iter().sum();
            self.sliding_avg_duration_ms = sum as f64 / self.recent_durations_ms.len() as f64;
        }

        if self.recent_memory_deltas.is_empty() {
            self.sliding_avg_memory_delta_bytes = 0.0;
        } else {
            let sum: isize = self.recent_memory_deltas.iter().sum();
            self.sliding_avg_memory_delta_bytes =
                sum as f64 / self.recent_memory_deltas.len() as f64;
        }
    }

    pub fn add_call(&mut self, duration_ms: u64, memory_delta_bytes: isize, is_success: bool) {
        self.total_calls += 1;

        self.min_duration_ms = self.min_duration_ms.min(duration_ms);
        self.max_duration_ms = self.max_duration_ms.max(duration_ms);
        let total_duration =
            self.avg_duration_ms * (self.total_calls - 1) as f64 + duration_ms as f64;
        self.avg_duration_ms = total_duration / self.total_calls as f64;

        self.total_memory_delta_bytes += memory_delta_bytes;
        self.min_memory_delta_bytes = self.min_memory_delta_bytes.min(memory_delta_bytes);
        self.max_memory_delta_bytes = self.max_memory_delta_bytes.max(memory_delta_bytes);
        self.avg_memory_delta_bytes =
            self.total_memory_delta_bytes as f64 / self.total_calls as f64;

        if is_success {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }

        if self.recent_durations_ms.len() >= SLIDING_WINDOW_SIZE {
            self.recent_durations_ms.pop_front();
        }
        self.recent_durations_ms.push_back(duration_ms);

        if self.recent_memory_deltas.len() >= SLIDING_WINDOW_SIZE {
            self.recent_memory_deltas.pop_front();
        }
        self.recent_memory_deltas.push_back(memory_delta_bytes);

        self.update_sliding_averages();
    }
}
