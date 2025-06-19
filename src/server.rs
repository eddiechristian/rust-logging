use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use log::info;
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub request_count: Arc<RwLock<u64>>,
    pub service_name: String,
    pub version: String,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            request_count: Arc::new(RwLock::new(0)),
            service_name: "axum-health-service".to_string(),
            version: "0.1.0".to_string(),
        }
    }
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    service_name: String,
    version: String,
    request_count: u64,
}

async fn health(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<HealthResponse>, StatusCode> {
    info!("Health endpoint called from client: {}", addr);
    info!("Client IP: {}, Client Port: {}", addr.ip(), addr.port());
    
    // Increment request counter
    let mut count = state.request_count.write().await;
    *count += 1;
    let current_count = *count;
    drop(count); // Release the lock early
    
    let response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        service_name: state.service_name.clone(),
        version: state.version.clone(),
        request_count: current_count,
    };
    
    info!("Health check successful for client {}: {:?}", addr, response.status);
    info!("Total health check requests: {}", current_count);
    Ok(Json(response))
}

pub fn create_router() -> Router {
    let state = AppState::new();
    
    Router::new()
        .route("/health", get(health))
        .with_state(state)
}

