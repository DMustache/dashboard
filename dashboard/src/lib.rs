use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Error,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
    time::{Duration, Instant, SystemTime},
};

use crate::models::{
    MemoryAllocation, MemoryStats, PersistentMetricsData, RealtimeMetricsData, RouteCall,
    RouteStats, SystemSnapshot, TrackingAllocator,
};
use axum::{
    extract::{ws::WebSocket, Json as AxumJson, State, WebSocketUpgrade},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sysinfo::System as SysInfoSystem;
use tokio::sync::broadcast;
pub mod globals;
mod models;

const SLIDING_WINDOW_SIZE: usize = 50;

const PERSISTENCE_FILE: &str = "dashboard_metrics.json";

static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_MEMORY_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_MEMORY_BYTES: AtomicUsize = AtomicUsize::new(0);

tokio::task_local! {
    static REQUEST_MEMORY_DELTA: std::cell::RefCell<(usize, usize)>;
}

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

fn collect_system_snapshot() -> SystemSnapshot {
    let pid = globals::process_pid();
    let mut sys = globals::sysinfo_instance().lock().unwrap();
    sys.refresh_all();

    let maybe_process = sys.process(pid);
    let process_cpu = maybe_process.map_or(0.0, |process| process.cpu_usage());
    let process_mem = maybe_process.map_or(0, |process| process.memory());

    SystemSnapshot {
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        cpu_usage: sys.global_cpu_usage(),
        memory_usage: sys.used_memory(),
        process_cpu_usage: process_cpu,
        process_memory_usage: process_mem,
        allocations: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocations: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        current_memory: CURRENT_MEMORY_BYTES.load(Ordering::Relaxed),
        peak_memory: PEAK_MEMORY_BYTES.load(Ordering::Relaxed),
    }
}

fn update_memory_stats_and_events() {
    let allocated = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let deallocated = DEALLOCATED_BYTES.load(Ordering::Relaxed);
    let current_memory = CURRENT_MEMORY_BYTES.load(Ordering::Relaxed);
    let peak_memory = PEAK_MEMORY_BYTES.load(Ordering::Relaxed);

    if let Ok(mut stats) = globals::memory_stats().write() {
        stats.total_allocations = allocated;
        stats.total_deallocations = deallocated;
        stats.current_memory = current_memory;
        stats.peak_memory = peak_memory;
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let stack_trace = None;

    let memory_event = MemoryAllocation {
        size: 0,
        timestamp,
        stack_trace,
        allocation_type: "periodic_update".to_owned(),
    };
    globals::add_to_deque(globals::memory_events(), memory_event);
}

fn collect_realtime_metrics() -> RealtimeMetricsData {
    let system_snapshot = collect_system_snapshot();
    globals::add_to_deque(globals::system_snapshots(), system_snapshot.clone());

    let mut route_stats_clone = globals::route_stats().read().unwrap().clone();
    for stats in route_stats_clone.values_mut() {
        stats.update_sliding_averages();
    }

    RealtimeMetricsData {
        system_snapshot,
        recent_route_calls: globals::get_recent_from_deque(globals::route_calls(), 50),
        route_stats: route_stats_clone,
        recent_memory_events: globals::get_recent_from_deque(globals::memory_events(), 50),
        memory_stats: globals::memory_stats().read().unwrap().clone(),
        blacklist: globals::blacklist()
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect(),
    }
}

pub async fn route_tracker(
    State(app_state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let route_key = format!("{method} {path}");

    if app_state.blacklist.read().unwrap().contains(&route_key) {
        return next.run(request).await;
    }

    let start_time = Instant::now();
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = REQUEST_MEMORY_DELTA
        .scope(std::cell::RefCell::new((0, 0)), async {
            next.run(request).await
        })
        .await;

    let (allocated_during_req, deallocated_during_req) = REQUEST_MEMORY_DELTA
        .try_with(|delta| *delta.borrow())
        .unwrap_or((0, 0));
    let memory_delta_bytes = allocated_during_req as isize - deallocated_during_req as isize;

    let duration_ms = u64::try_from(start_time.elapsed().as_millis()).unwrap();
    let status_code = response.status().as_u16();
    let is_success = status_code < 400;

    let route_call = RouteCall {
        path: path.clone(),
        method: method.clone(),
        timestamp,
        duration_ms: Some(duration_ms),
        status_code: Some(status_code),
        memory_delta_bytes: Some(memory_delta_bytes),
    };

    {
        let mut stats_map = app_state.route_stats.write().unwrap();
        let stats = stats_map.entry(route_key.clone()).or_default();
        stats.add_call(duration_ms, memory_delta_bytes, is_success);
    }

    globals::add_to_deque(app_state.route_calls, route_call);

    if let Ok(tx_guard) = app_state.metrics_tx.lock() {
        if let Some(tx) = tx_guard.as_ref() {
            if tx.receiver_count() > 0 {
                let metrics = collect_realtime_metrics();
                if let Ok(json) = serde_json::to_string(&metrics) {
                    let _ = tx.send(json);
                }
            }
        }
    }

    response
}

fn save_metrics_to_file(path: &str, data: &PersistentMetricsData) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(data)
        .map_err(|error| Error::new(std::io::ErrorKind::Other, error))?;
    fs::write(path, json)
}

fn load_metrics_from_file(path: &str) -> std::io::Result<PersistentMetricsData> {
    if !Path::new(path).exists() {
        return Ok(PersistentMetricsData::default());
    }
    let json = fs::read_to_string(path)?;
    serde_json::from_str(&json)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn merge_loaded_metrics(loaded_data: PersistentMetricsData) {
    if let Ok(mut current_snapshots) = globals::system_snapshots().lock() {
        let mut combined: Vec<_> = loaded_data
            .system_snapshots
            .into_iter()
            .chain(current_snapshots.iter().cloned())
            .collect();
        combined.sort_by_key(|system_snapshot| system_snapshot.timestamp);
        combined.dedup_by_key(|system_snapshot| system_snapshot.timestamp);
        let start_index = if combined.len() > globals::MAX_HISTORY {
            combined.len() - globals::MAX_HISTORY
        } else {
            0
        };
        *current_snapshots = combined.into_iter().skip(start_index).collect();
    }

    if let Ok(mut current_calls) = globals::route_calls().lock() {
        let mut combined: Vec<_> = loaded_data
            .route_calls
            .into_iter()
            .chain(current_calls.iter().cloned())
            .collect();
        combined.sort_by_key(|route_call| route_call.timestamp);
        combined.dedup_by(|a_route_call, b_route_call| {
            a_route_call.timestamp == b_route_call.timestamp
                && a_route_call.path == b_route_call.path
                && a_route_call.method == b_route_call.method
        });
        let start_index = if combined.len() > globals::MAX_HISTORY {
            combined.len() - globals::MAX_HISTORY
        } else {
            0
        };
        *current_calls = combined.into_iter().skip(start_index).collect();
    }

    if let Ok(mut current_events) = globals::memory_events().lock() {
        let mut combined: Vec<_> = loaded_data
            .memory_events
            .into_iter()
            .chain(current_events.iter().cloned())
            .collect();
        combined.sort_by_key(|memory_allocation| memory_allocation.timestamp);
        combined.dedup_by_key(|memory_allocation| memory_allocation.timestamp);
        let start_index = if combined.len() > globals::MAX_HISTORY {
            combined.len() - globals::MAX_HISTORY
        } else {
            0
        };
        *current_events = combined.into_iter().skip(start_index).collect();
    }

    if let Ok(mut current_stats_map) = globals::route_stats().write() {
        for (route_key, loaded_stats) in loaded_data.route_stats {
            let current_stats = current_stats_map.entry(route_key).or_default();

            let new_total_calls = current_stats.total_calls + loaded_stats.total_calls;
            if new_total_calls > 0 {
                current_stats.avg_duration_ms = ((current_stats.avg_duration_ms
                    * current_stats.total_calls as f64)
                    + (loaded_stats.avg_duration_ms * loaded_stats.total_calls as f64))
                    / new_total_calls as f64;

                current_stats.avg_memory_delta_bytes = ((current_stats.avg_memory_delta_bytes
                    * current_stats.total_calls as f64)
                    + (loaded_stats.avg_memory_delta_bytes * loaded_stats.total_calls as f64))
                    / new_total_calls as f64;
            }

            current_stats.total_calls = new_total_calls;
            current_stats.success_count += loaded_stats.success_count;
            current_stats.error_count += loaded_stats.error_count;
            current_stats.total_memory_delta_bytes += loaded_stats.total_memory_delta_bytes;

            current_stats.min_duration_ms = current_stats
                .min_duration_ms
                .min(loaded_stats.min_duration_ms);
            current_stats.max_duration_ms = current_stats
                .max_duration_ms
                .max(loaded_stats.max_duration_ms);
            current_stats.min_memory_delta_bytes = current_stats
                .min_memory_delta_bytes
                .min(loaded_stats.min_memory_delta_bytes);
            current_stats.max_memory_delta_bytes = current_stats
                .max_memory_delta_bytes
                .max(loaded_stats.max_memory_delta_bytes);

            if current_stats.recent_durations_ms.is_empty() {
                current_stats.recent_durations_ms = loaded_stats.recent_durations_ms;
            }
            if current_stats.recent_memory_deltas.is_empty() {
                current_stats.recent_memory_deltas = loaded_stats.recent_memory_deltas;
            }
            current_stats.update_sliding_averages();
        }
    }

    if let Ok(mut current_memory_stats) = globals::memory_stats().write() {
        current_memory_stats.peak_memory = current_memory_stats
            .peak_memory
            .max(loaded_data.memory_stats.peak_memory);

        for (size_cat, count) in loaded_data.memory_stats.allocation_sizes {
            *current_memory_stats
                .allocation_sizes
                .entry(size_cat)
                .or_insert(0) += count;
        }
    }

    if let Ok(mut current_blacklist) = globals::blacklist().write() {
        current_blacklist.extend(loaded_data.blacklist);
    }
}

#[derive(Clone)]
pub struct AppState {
    route_calls: &'static Arc<Mutex<VecDeque<RouteCall>>>,
    route_stats: &'static Arc<RwLock<HashMap<String, RouteStats>>>,
    blacklist: &'static Arc<RwLock<HashSet<String>>>,
    metrics_tx: &'static Mutex<Option<broadcast::Sender<String>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            route_calls: globals::route_calls(),
            route_stats: globals::route_stats(),
            blacklist: globals::blacklist(),
            metrics_tx: globals::metrics_tx(),
        }
    }
}

async fn dashboard_handler() -> Html<String> {
    Html(fs::read_to_string("./ui/index.html").unwrap())
}

async fn ws_handler(
    web_socket: WebSocketUpgrade,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    web_socket.on_upgrade(|socket| handle_socket(socket, app_state))
}

async fn handle_socket(mut web_socket: WebSocket, app_state: AppState) {
    let (sender, mut receiver) = broadcast::channel(100);
    {
        let mut reciver_guard = app_state.metrics_tx.lock().unwrap();
        *reciver_guard = Some(sender.clone());
    }

    let initial_metrics = collect_realtime_metrics();
    if let Ok(json) = serde_json::to_string(&initial_metrics) {
        if web_socket.send(json.into()).await.is_err() {
            let mut reciever_guard = app_state.metrics_tx.lock().unwrap();
            *reciever_guard = None;
            return;
        }
    }

    let update_tx = sender.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            update_memory_stats_and_events();
            let metrics = collect_realtime_metrics();
            if let Ok(json) = serde_json::to_string(&metrics) {
                if update_tx.send(json).is_err() {
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            message = receiver.recv() => {
                match message {
                    Ok(json) => {
                        if web_socket.send(json.into()).await.is_err() {

                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            else => break,
        }
    }

    let mut tx_guard = app_state.metrics_tx.lock().unwrap();
    *tx_guard = None;
}

async fn save_metrics_handler() -> impl IntoResponse {
    let data = PersistentMetricsData::collect();
    match save_metrics_to_file(PERSISTENCE_FILE, &data) {
        Ok(()) => (StatusCode::OK, "Metrics saved").into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save metrics: {error}"),
        )
            .into_response(),
    }
}

async fn load_metrics_handler() -> impl IntoResponse {
    match load_metrics_from_file(PERSISTENCE_FILE) {
        Ok(data) => {
            merge_loaded_metrics(data);
            if let Ok(tx_guard) = globals::metrics_tx().lock() {
                if let Some(tx) = tx_guard.as_ref() {
                    let metrics = collect_realtime_metrics();
                    if let Ok(json) = serde_json::to_string(&metrics) {
                        let _ = tx.send(json);
                    }
                }
            }
            (StatusCode::OK, "Metrics loaded and merged").into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load metrics: {error}"),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct DeleteMetricsPayload {
    start_time: u64,
    end_time: u64,
}

async fn delete_metrics_handler(
    AxumJson(payload): AxumJson<DeleteMetricsPayload>,
) -> impl IntoResponse {
    match load_metrics_from_file(PERSISTENCE_FILE) {
        Ok(mut data) => {
            data.system_snapshots.retain(|system_snapshot| {
                system_snapshot.timestamp < payload.start_time
                    || system_snapshot.timestamp >= payload.end_time
            });
            data.route_calls.retain(|route_call| {
                route_call.timestamp < payload.start_time
                    || route_call.timestamp >= payload.end_time
            });
            data.memory_events.retain(|memory_allocation| {
                memory_allocation.timestamp < payload.start_time
                    || memory_allocation.timestamp >= payload.end_time
            });

            match save_metrics_to_file(PERSISTENCE_FILE, &data) {
                Ok(()) => {
                    if let Ok(reloaded_data) = load_metrics_from_file(PERSISTENCE_FILE) {
                        if let Ok(mut snapshots) = globals::system_snapshots().lock() {
                            snapshots.clear();
                        }
                        if let Ok(mut calls) = globals::route_calls().lock() {
                            calls.clear();
                        }
                        if let Ok(mut events) = globals::memory_events().lock() {
                            events.clear();
                        }
                        if let Ok(mut stats) = globals::route_stats().write() {
                            stats.clear();
                        }
                        if let Ok(mut memory_stats) = globals::memory_stats().write() {
                            *memory_stats = MemoryStats::default();
                        }

                        merge_loaded_metrics(reloaded_data);
                    }

                    if let Ok(tx_guard) = globals::metrics_tx().lock() {
                        if let Some(tx) = tx_guard.as_ref() {
                            let metrics = collect_realtime_metrics();
                            if let Ok(json) = serde_json::to_string(&metrics) {
                                let _ = tx.send(json);
                            }
                        }
                    }
                    (StatusCode::OK, "Metrics data deleted").into_response()
                }
                Err(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to save updated metrics: {error}"),
                )
                    .into_response(),
            }
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load metrics for deletion: {error}"),
        )
            .into_response(),
    }
}

#[derive(Deserialize, Serialize)]
struct BlacklistItemPayload {
    item: String,
}

async fn get_blacklist_handler(State(app_state): State<AppState>) -> Json<Vec<String>> {
    let list = app_state
        .blacklist
        .read()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    Json(list)
}

async fn add_blacklist_item_handler(
    State(app_state): State<AppState>,
    AxumJson(payload): AxumJson<BlacklistItemPayload>,
) -> impl IntoResponse {
    let mut is_added = false;
    if let Ok(mut blacklist) = app_state.blacklist.write() {
        is_added = blacklist.insert(payload.item);
    }
    if is_added {
        if let Ok(tx_guard) = globals::metrics_tx().lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let metrics = collect_realtime_metrics();
                if let Ok(json) = serde_json::to_string(&metrics) {
                    let _ = tx.send(json);
                }
            }
        }
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    }
}

async fn remove_blacklist_item_handler(
    State(app_state): State<AppState>,
    AxumJson(payload): AxumJson<BlacklistItemPayload>,
) -> impl IntoResponse {
    let mut removed = false;
    if let Ok(mut blacklist) = app_state.blacklist.write() {
        removed = blacklist.remove(&payload.item);
    }
    if removed {
        if let Ok(tx_guard) = globals::metrics_tx().lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let metrics = collect_realtime_metrics();
                if let Ok(json) = serde_json::to_string(&metrics) {
                    let _ = tx.send(json);
                }
            }
        }
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub fn create_metrics_router() -> Router {
    if let Ok(data) = load_metrics_from_file(PERSISTENCE_FILE) {
        merge_loaded_metrics(data);
    }

    let app_state = AppState::new();

    Router::new()
        .route("/metrics", get(dashboard_handler))
        .route("/metrics-ws", get(ws_handler))
        .route("/save-metrics", post(save_metrics_handler))
        .route("/load-metrics", post(load_metrics_handler))
        .route("/delete-metrics", post(delete_metrics_handler))
        .route("/blacklist", get(get_blacklist_handler))
        .route("/blacklist/add", post(add_blacklist_item_handler))
        .route("/blacklist/remove", post(remove_blacklist_item_handler))
        .with_state(app_state)
}

pub fn instrument_routes(router: Router) -> Router {
    router.layer(middleware::from_fn_with_state(
        AppState::new(),
        route_tracker,
    ))
}

pub fn register_shutdown_handler<F>(mut shutdown_handler: F)
where
    F: FnMut() + Send + Sync + 'static,
{
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let _ = ctrlc::set_handler(move || {
            println!("Shutdown signal received. Saving metrics...");
            let data = PersistentMetricsData::collect();
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let path = format!("metrics_shutdown_{timestamp}.json");
            if let Err(error) = save_metrics_to_file(&path, &data) {
                eprintln!("Failed to save metrics on shutdown: {error}");
            }
            shutdown_handler();
            std::process::exit(0);
        });
    });
}

pub fn auto_save_metrics_on_shutdown() {
    register_shutdown_handler(|| {});
}
