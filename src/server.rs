use anyhow::{Context, Result};
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
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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

#[derive(Deserialize)]
pub struct HbdParams {
    #[serde(alias = "ID", alias = "id", alias = "Id")]
    pub id: i32,
    #[serde(alias = "MAC", alias = "mac", alias = "Mac")]
    pub mac: String,
    #[serde(alias = "IP", alias = "ip", alias = "Ip")]
    pub ip: String,
    #[serde(alias = "LP", alias = "lp", alias = "Lp")]
    pub lp: Option<i32>,
    #[serde(alias = "ts", alias = "TS", alias = "Ts")]
    pub ts: Option<i64>, // timestamp as number (Unix timestamp)
}

#[derive(Serialize)]
pub struct HbdResponse {
    pub status: String,
    pub message: String,
    pub received_data: HbdData,
    pub processed_at: String,
}

#[derive(Serialize)]
pub struct HbdData {
    pub id: i32,
    pub mac: String,
    pub ip: String,
    pub lp: Option<i32>,
    pub timestamp: Option<i64>,
    pub timestamp_iso: Option<String>, // Human-readable timestamp
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    service_name: String,
    version: String,
    health_count: u64,
    user_agent: Option<String>,
    headers_count: usize,
    database_status: String,
}

async fn health(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<HealthResponse>, StatusCode> {
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

    // Increment health counter
    let current_count = state.health_count.fetch_add(1) + 1;

    // Check database health
    let db_healthy = state.is_db_healthy();
    let database_status = if db_healthy {
        "connected".to_string()
    } else {
        "disconnected".to_string()
    };

    let overall_status = if db_healthy {
        "healthy".to_string()
    } else {
        "degraded".to_string()
    };

    let response = HealthResponse {
        status: overall_status,
        timestamp: chrono::Utc::now().to_rfc3339(),
        service_name: state.service_name.clone(),
        version: state.version.clone(),
        health_count: current_count,
        user_agent,
        headers_count: headers.len(),
        database_status,
    };

    info!(
        "Health check successful for client {}: {:?}",
        addr, response.status
    );
    info!("Total health check requests: {}", current_count);
    Ok(Json(response))
}

async fn hbd(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HbdParams>,
) -> Result<Json<HbdResponse>, StatusCode> {
    info!("HBD endpoint called from client: {}", addr);
    info!(
        "HBD Parameters - ID: {}, MAC: {}, IP: {}, LP: {:?}, TS: {:?}",
        params.id, params.mac, params.ip, params.lp, params.ts
    );

    // Validate timestamp if provided (check if it's a reasonable Unix timestamp)
    // Allow timestamps from year 2000 (946684800) to year 2100 (4102444800)
    let timestamp_iso = if let Some(ts) = params.ts {
        if ts < 946684800 || ts > 4102444800 {
            info!("Invalid timestamp range: {}", ts);
            return Err(StatusCode::BAD_REQUEST);
        }

        // Convert Unix timestamp to ISO format for human readability
        match chrono::DateTime::from_timestamp(ts, 0) {
            Some(dt) => Some(dt.to_rfc3339()),
            None => {
                info!("Failed to convert timestamp to ISO format: {}", ts);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        None
    };

    // Increment HBD counter
    let current_count = state.hbd_count.fetch_add(1) + 1;

    let response = HbdResponse {
        status: "success".to_string(),
        message: "Heartbeat data received and processed".to_string(),
        received_data: HbdData {
            id: params.id,
            mac: params.mac,
            ip: params.ip,
            lp: params.lp,
            timestamp: params.ts,
            timestamp_iso,
        },
        processed_at: chrono::Utc::now().to_rfc3339(),
    };

    info!("HBD data processed successfully for client {}", addr);
    info!("Total HBD requests processed: {}", current_count);
    Ok(Json(response))
}

pub fn create_router(db_pool: Pool) -> Router {
    let state = AppState::new(db_pool);

    Router::new()
        .route("/health", get(health))
        .route("/hbd", get(hbd))
        .with_state(state)
}
