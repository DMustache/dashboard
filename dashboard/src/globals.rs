use crate::models::{MemoryAllocation, MemoryStats, RouteCall, RouteStats, SystemSnapshot};

use super::{
    broadcast, Arc, Deserialize, HashMap, HashSet, Mutex, OnceLock, RwLock, Serialize,
    SysInfoSystem, VecDeque,
};

static SYSTEM_SNAPSHOTS: OnceLock<Arc<Mutex<VecDeque<SystemSnapshot>>>> = OnceLock::new();
static ROUTE_CALLS: OnceLock<Arc<Mutex<VecDeque<RouteCall>>>> = OnceLock::new();
static ROUTE_STATS: OnceLock<Arc<RwLock<HashMap<String, RouteStats>>>> = OnceLock::new();
static MEMORY_EVENTS: OnceLock<Arc<Mutex<VecDeque<MemoryAllocation>>>> = OnceLock::new();
static MEMORY_STATS: OnceLock<Arc<RwLock<MemoryStats>>> = OnceLock::new();
static BLACKLIST: OnceLock<Arc<RwLock<HashSet<String>>>> = OnceLock::new();
static METRICS_TX: OnceLock<Mutex<Option<broadcast::Sender<String>>>> = OnceLock::new();
static SYSINFO: OnceLock<Mutex<SysInfoSystem>> = OnceLock::new();
static PROCESS_PID: OnceLock<sysinfo::Pid> = OnceLock::new();

pub const MAX_HISTORY: usize = 1000;

fn get_or_init_deque<
    T: Default + Send + Sync + Clone + Serialize + for<'de> Deserialize<'de> + 'static,
>(
    lock: &'static OnceLock<Arc<Mutex<VecDeque<T>>>>,
) -> &'static Arc<Mutex<VecDeque<T>>> {
    lock.get_or_init(|| Arc::new(Mutex::new(VecDeque::with_capacity(MAX_HISTORY))))
}

fn get_or_init_map<K, V>(
    lock: &'static OnceLock<Arc<RwLock<HashMap<K, V>>>>,
) -> &'static Arc<RwLock<HashMap<K, V>>>
where
    K: Eq + std::hash::Hash + Send + Sync + 'static,
    V: Default + Send + Sync + 'static,
{
    lock.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

pub fn system_snapshots() -> &'static Arc<Mutex<VecDeque<SystemSnapshot>>> {
    get_or_init_deque(&SYSTEM_SNAPSHOTS)
}

pub fn route_calls() -> &'static Arc<Mutex<VecDeque<RouteCall>>> {
    get_or_init_deque(&ROUTE_CALLS)
}

pub fn route_stats() -> &'static Arc<RwLock<HashMap<String, RouteStats>>> {
    get_or_init_map(&ROUTE_STATS)
}

pub fn memory_events() -> &'static Arc<Mutex<VecDeque<MemoryAllocation>>> {
    get_or_init_deque(&MEMORY_EVENTS)
}

pub fn memory_stats() -> &'static Arc<RwLock<MemoryStats>> {
    MEMORY_STATS.get_or_init(|| Arc::new(RwLock::new(MemoryStats::default())))
}

pub fn blacklist() -> &'static Arc<RwLock<HashSet<String>>> {
    BLACKLIST.get_or_init(|| Arc::new(RwLock::new(HashSet::new())))
}

pub fn metrics_tx() -> &'static Mutex<Option<broadcast::Sender<String>>> {
    METRICS_TX.get_or_init(|| Mutex::new(None))
}

pub fn sysinfo_instance() -> &'static Mutex<SysInfoSystem> {
    SYSINFO.get_or_init(|| {
        let mut system = SysInfoSystem::new_all();
        system.refresh_all();
        Mutex::new(system)
    })
}

pub fn process_pid() -> sysinfo::Pid {
    *PROCESS_PID.get_or_init(|| sysinfo::Pid::from(std::process::id() as usize))
}

pub fn add_to_deque<T>(deque_lock: &Arc<Mutex<VecDeque<T>>>, item: T) {
    if let Ok(mut deque) = deque_lock.lock() {
        if deque.len() >= MAX_HISTORY {
            deque.pop_front();
        }
        deque.push_back(item);
    }
}

pub fn get_recent_from_deque<T: Clone>(
    deque_lock: &Arc<Mutex<VecDeque<T>>>,
    count: usize,
) -> Vec<T> {
    if let Ok(deque) = deque_lock.lock() {
        deque.iter().rev().take(count).cloned().collect()
    } else {
        Vec::new()
    }
}
