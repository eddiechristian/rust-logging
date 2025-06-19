use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::get,
    Router,
};
use crossbeam::atomic::AtomicCell;
use log::info;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr};

#[derive(Clone)]
pub struct AppState {
    pub health_count: AtomicCell<u64>,
    pub hbd_count: AtomicCell<u64>,
    pub service_name: String,
    pub version: String,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            health_count: AtomicCell::new(0),
            hbd_count: AtomicCell::new(0),
            service_name: "axum-health-service".to_string(),
            version: "0.1.0".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct HbdParams {
    #[serde(alias = "ID", alias = "id", alias = "Id")]
    id: i32,
    #[serde(alias = "MAC", alias = "mac", alias = "Mac")]
    mac: String,
    #[serde(alias = "IP", alias = "ip", alias = "Ip")]
    ip: String,
    #[serde(alias = "LP", alias = "lp", alias = "Lp")]
    lp: Option<i32>,
    #[serde(alias = "ts", alias = "TS", alias = "Ts")]
    ts: Option<i64>, // timestamp as number (Unix timestamp)
}

#[derive(Serialize)]
struct HbdResponse {
    status: String,
    message: String,
    received_data: HbdData,
    processed_at: String,
    hbd_count: u64,
}

#[derive(Serialize)]
struct HbdData {
    id: i32,
    mac: String,
    ip: String,
    lp: Option<i32>,
    timestamp: Option<i64>,
    timestamp_iso: Option<String>, // Human-readable timestamp
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
    
    let response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        service_name: state.service_name.clone(),
        version: state.version.clone(),
        health_count: current_count,
        user_agent,
        headers_count: headers.len(),
    };
    
    info!("Health check successful for client {}: {:?}", addr, response.status);
    info!("Total health check requests: {}", current_count);
    Ok(Json(response))
}

async fn hbd(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HbdParams>,
) -> Result<Json<HbdResponse>, StatusCode> {
    info!("HBD endpoint called from client: {}", addr);
    info!("HBD Parameters - ID: {}, MAC: {}, IP: {}, LP: {:?}, TS: {:?}", 
        params.id, params.mac, params.ip, params.lp, params.ts);
    
    // Validate MAC address format (basic validation)
    if params.mac.len() != 17 || !params.mac.chars().enumerate().all(|(i, c)| {
        if (i + 1) % 3 == 0 && i != 16 {
            c == ':' || c == '-'
        } else {
            c.is_ascii_hexdigit()
        }
    }) {
        info!("Invalid MAC address format: {}", params.mac);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Validate IP address format (basic validation)
    if params.ip.parse::<std::net::IpAddr>().is_err() {
        info!("Invalid IP address format: {}", params.ip);
        return Err(StatusCode::BAD_REQUEST);
    }
    
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
        hbd_count: current_count,
    };
    
    info!("HBD data processed successfully for client {}", addr);
    info!("Total HBD requests processed: {}", current_count);
    Ok(Json(response))
}

pub fn create_router() -> Router {
    let state = AppState::new();
    
    Router::new()
        .route("/health", get(health))
        .route("/hbd", get(hbd))
        .with_state(state)
}

