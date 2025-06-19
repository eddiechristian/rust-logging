use axum::{
    extract::ConnectInfo,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use log::info;
use serde::Serialize;
use std::net::SocketAddr;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
}

async fn health(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> Result<Json<HealthResponse>, StatusCode> {
    info!("Health endpoint called from client: {}", addr);
    info!("Client IP: {}, Client Port: {}", addr.ip(), addr.port());
    
    let response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    
    info!("Health check successful for client {}: {:?}", addr, response.status);
    Ok(Json(response))
}

pub fn create_router() -> Router {
    Router::new()
        .route("/health", get(health))
}

