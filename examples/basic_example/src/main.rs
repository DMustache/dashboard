use axum::{
    extract::{Json, Path},
    http::StatusCode,
    routing::get,
    Router,
};
use dashboard::{auto_save_metrics_on_shutdown, create_metrics_router, instrument_routes};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Clone)]
struct AppState {
    users: Arc<RwLock<HashMap<u64, User>>>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        users: Arc::new(RwLock::new(HashMap::new())),
    };

    let app_routes = Router::new()
        .without_v07_checks()
        .route("/", get(index))
        .route("/users", get(get_users).post(create_user))
        .route("/users/:id", get(get_user).delete(delete_user))
        .route("/heavy", get(heavy_computation))
        .route("/memory", get(memory_intensive))
        .with_state(state);

    let metrics_routes = create_metrics_router();

    let app = Router::new()
        .without_v07_checks()
        .merge(instrument_routes(app_routes))
        .merge(metrics_routes);

    auto_save_metrics_on_shutdown();

    let address = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Сервер запущен на http://{address}");
    println!("Дашборд доступен на http://{address}/dashboard/metrics",);

    axum_server::bind(address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn index() -> &'static str {
    "Hello World!"
}

async fn get_users(state: axum::extract::State<AppState>) -> Json<Vec<User>> {
    let users = state.users.read().unwrap();
    let users_vec: Vec<User> = users.values().cloned().collect();
    Json(users_vec)
}

async fn create_user(
    state: axum::extract::State<AppState>,
    Json(user): Json<User>,
) -> (StatusCode, Json<User>) {
    let mut users = state.users.write().unwrap();
    users.insert(user.id, user.clone());
    (StatusCode::CREATED, Json(user))
}

async fn get_user(
    state: axum::extract::State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, StatusCode> {
    let users = state.users.read().unwrap();
    users
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_user(state: axum::extract::State<AppState>, Path(id): Path<u64>) -> StatusCode {
    let mut users = state.users.write().unwrap();
    if users.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn heavy_computation() -> String {
    let mut result = 0;
    for i in 0..1_000 {
        result += i;
    }
    format!("Результат вычислений: {result}")
}

async fn memory_intensive() -> String {
    let big_vec: Vec<u8> = vec![0; 10 * 1024 * 1024]; // 10 MB
    format!("Создан вектор размером {} байт", big_vec.len())
}
