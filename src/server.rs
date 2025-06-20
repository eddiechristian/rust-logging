use anyhow::Result;
use axum::{
    Router,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::get,
};
use crossbeam::atomic::AtomicCell;
use log::{error, info};
use mysql::{Pool, PooledConn};
use mysql::prelude::Queryable;
use std::net::SocketAddr;

use crate::app::{HbdParams, HbdService, HealthService};

pub struct AppState {
    pub health_count: AtomicCell<u64>,
    pub hbd_count: AtomicCell<u64>,
    pub service_name: String,
    pub version: String,
    pub db_pool: Pool,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            health_count: AtomicCell::new(self.health_count.load()),
            hbd_count: AtomicCell::new(self.hbd_count.load()),
            service_name: self.service_name.clone(),
            version: self.version.clone(),
            db_pool: self.db_pool.clone(),
        }
    }
}

impl AppState {
    pub fn new(db_pool: Pool) -> Self {
        Self {
            health_count: AtomicCell::new(0),
            hbd_count: AtomicCell::new(0),
            service_name: "axum-health-service".to_string(),
            version: "0.1.0".to_string(),
            db_pool,
        }
    }
    
    /// Get a database connection from the pool
    pub fn get_connection(&self) -> Result<PooledConn> {
        self.db_pool
            .get_conn()
            .map_err(|e| {
                error!("Failed to get database connection: {}", e);
                anyhow::anyhow!("Database connection failed: {}", e)
            })
    }
    
    /// Check if database connection is healthy
    pub fn is_db_healthy(&self) -> bool {
        match self.get_connection() {
            Ok(mut conn) => {
                // Try a simple query to test the connection
                match conn.query_drop("SELECT 1") {
                    Ok(_) => true,
                    Err(e) => {
                        error!("Database health check failed: {}", e);
                        false
                    }
                }
            }
            Err(_) => false,
        }
    }
}


async fn health(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Json<crate::app::HealthResponse> {
    info!("Health endpoint called from client: {}", addr);
    info!("Client IP: {}, Client Port: {}", addr.ip(), addr.port());

    // Log header information
    info!("Request headers count: {}", headers.len());

    // Extract User-Agent header
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(ref ua) = user_agent {
        info!("User-Agent: {}", ua);
    }

    // Log some common headers
    for (key, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            info!("Header {}: {}", key, value_str);
        }
    }

    // Delegate business logic to HealthService
    let response = HealthService::process_health_check(
        &state,
        addr,
        headers.len(),
        user_agent,
    );

    Json(response)
}

async fn hbd(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HbdParams>,
) -> Result<Json<crate::app::HbdResponse>, StatusCode> {
    info!("HBD endpoint called from client: {}", addr);
    info!(
        "HBD Parameters - ID: {}, MAC: {}, IP: {}, LP: {:?}, TS: {:?}",
        params.id, params.mac, params.ip, params.lp, params.ts
    );

    // Delegate business logic to HbdService
    match HbdService::process_heartbeat(&state, params, addr) {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("HBD processing failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

pub fn create_router(db_pool: Pool) -> Router {
    let state = AppState::new(db_pool);

    Router::new()
        .route("/health", get(health))
        .route("/hbd", get(hbd))
        .with_state(state)
}
