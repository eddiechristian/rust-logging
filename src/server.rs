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
use mysql::prelude::Queryable;
use mysql::{Pool, PooledConn};
use sp_stats_monitor::DetailedStatsMonitor;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use sysinfo::System;

use crate::app::{HbdParams, HbdService, HealthService};

/// CPU monitoring with lock-free reads using atomic cache
pub struct CpuMonitor {
    /// Cached CPU usage as percentage * 100 (so 45.67% becomes 4567)
    cpu_usage_percent_x100: AtomicU64,
}

impl CpuMonitor {
    pub fn new() -> Arc<Self> {
        let monitor = Arc::new(Self {
            cpu_usage_percent_x100: AtomicU64::new(0),
        });

        // Start background thread to update CPU stats
        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            let mut system = System::new_all();

            loop {
                // Refresh CPU data
                system.refresh_cpu();

                // Calculate average CPU usage across all cores
                let cpu_usage = if system.cpus().is_empty() {
                    0.0
                } else {
                    system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()
                        / system.cpus().len() as f32
                };

                // Store as integer (percentage * 100 for precision)
                let cpu_usage_x100 = (cpu_usage * 100.0) as u64;
                monitor_clone
                    .cpu_usage_percent_x100
                    .store(cpu_usage_x100, Ordering::Relaxed);

                // Update every 2 seconds
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        monitor
    }

    /// Get current CPU usage percentage (lock-free read)
    pub fn get_cpu_usage(&self) -> f64 {
        let cpu_usage_x100 = self.cpu_usage_percent_x100.load(Ordering::Relaxed);
        cpu_usage_x100 as f64 / 100.0
    }
}

pub struct AppState {
    pub health_count: AtomicCell<u64>,
    pub hbd_count: AtomicCell<u64>,
    pub service_name: String,
    pub version: String,
    pub db_pool: Pool,
    pub stats_monitor: Arc<DetailedStatsMonitor>,
    pub cpu_monitor: Arc<CpuMonitor>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            health_count: AtomicCell::new(self.health_count.load()),
            hbd_count: AtomicCell::new(self.hbd_count.load()),
            service_name: self.service_name.clone(),
            version: self.version.clone(),
            db_pool: self.db_pool.clone(),
            stats_monitor: self.stats_monitor.clone(),
            cpu_monitor: self.cpu_monitor.clone(),
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
            stats_monitor: Arc::new(DetailedStatsMonitor::new()),
            cpu_monitor: CpuMonitor::new(),
        }
    }

    /// Get a database connection from the pool with session variables configured
    pub fn get_connection(&self) -> Result<PooledConn> {
        let mut conn = self.db_pool.get_conn().map_err(|e| {
            error!("Failed to get database connection: {}", e);
            anyhow::anyhow!("Database connection failed: {}", e)
        })?;

        // Configure session variables for this connection
        Self::configure_connection_session(&mut conn)?;

        Ok(conn)
    }

    /// Configure MySQL session variables for a connection
    fn configure_connection_session(conn: &mut PooledConn) -> Result<()> {
        // Set InnoDB lock wait timeout to 3 seconds
        conn.query_drop("SET SESSION innodb_lock_wait_timeout = 3")
            .map_err(|e| {
                error!("Failed to set innodb_lock_wait_timeout: {}", e);
                anyhow::anyhow!("Failed to configure session: {}", e)
            })?;

        // Set general wait timeout to 60 seconds
        conn.query_drop("SET SESSION wait_timeout = 60")
            .map_err(|e| {
                error!("Failed to set wait_timeout: {}", e);
                anyhow::anyhow!("Failed to configure session: {}", e)
            })?;

        Ok(())
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
    let start_time = Instant::now();

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
    let response = HealthService::process_health_check(&state, addr, headers.len(), user_agent);

    // Record web request performance for /health endpoint
    let request_duration = start_time.elapsed();
    state
        .stats_monitor
        .record_web_request("/health", request_duration);

    info!(
        "Health check completed in {:.2}ms",
        request_duration.as_secs_f64() * 1000.0
    );

    Json(response)
}

async fn hbd(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HbdParams>,
) -> Result<Json<crate::app::HbdResponse>, StatusCode> {
    let start_time = Instant::now();

    info!("HBD endpoint called from client: {}", addr);
    info!(
        "HBD Parameters - ID: {}, MAC: {}, IP: {}, LP: {:?}, TS: {:?}",
        params.id, params.mac, params.ip, params.lp, params.ts
    );

    // Delegate business logic to HbdService
    let result = match HbdService::process_heartbeat(&state, params, addr) {
        Ok(response) => {
            // Record web request performance for /hbd endpoint
            let request_duration = start_time.elapsed();
            state
                .stats_monitor
                .record_web_request("/hbd", request_duration);
            info!(
                "HBD request completed in {:.2}ms",
                request_duration.as_secs_f64() * 1000.0
            );

            Ok(Json(response))
        }
        Err(e) => {
            error!("HBD processing failed: {}", e);

            // Still record the request timing even for errors
            let request_duration = start_time.elapsed();
            state
                .stats_monitor
                .record_web_request("/hbd", request_duration);

            Err(StatusCode::BAD_REQUEST)
        }
    };

    result
}

/// Get current performance statistics
async fn stats(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();
    info!("Stats endpoint called from client: {}", addr);

    let (web_detailed_stats, db_detailed_stats) = state.stats_monitor.get_detailed_stats();
    let (web_agg_stats, db_agg_stats) = state.stats_monitor.get_aggregated_stats();

    // Get current CPU usage (lock-free read)
    let cpu_usage = state.cpu_monitor.get_cpu_usage();

    let stats_response = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": {
            "name": state.service_name,
            "version": state.version
        },
        "system_metrics": {
            "cpu_usage_percent": format!("{:.2}", cpu_usage)
        },
        "request_counters": {
            "health_checks": state.health_count.load(),
            "heartbeats": state.hbd_count.load()
        },
        "performance_metrics": {
            "web_endpoints": web_detailed_stats,
            "database_queries": db_detailed_stats,
            "aggregated": {
                "web_requests": {
                    "count": web_agg_stats.count,
                    "min_ms": if web_agg_stats.count > 0 { web_agg_stats.min_ms } else { 0.0 },
                    "max_ms": web_agg_stats.max_ms,
                    "mean_ms": web_agg_stats.mean_ms,
                    "total_ms": web_agg_stats.total_ms
                },
                "database_queries": {
                    "count": db_agg_stats.count,
                    "min_ms": if db_agg_stats.count > 0 { db_agg_stats.min_ms } else { 0.0 },
                    "max_ms": db_agg_stats.max_ms,
                    "mean_ms": db_agg_stats.mean_ms,
                    "total_ms": db_agg_stats.total_ms
                }
            }
        },
        "analysis": {
            "total_operations": web_agg_stats.count + db_agg_stats.count,
            "db_slower_than_web_ratio": if web_agg_stats.mean_ms > 0.0 && db_agg_stats.mean_ms > 0.0 {
                db_agg_stats.mean_ms / web_agg_stats.mean_ms
            } else {
                0.0
            },
            "tracked_endpoints": state.stats_monitor.get_tracked_endpoints(),
            "tracked_queries": state.stats_monitor.get_tracked_queries()
        }
    });

    // Record stats endpoint performance
    let request_duration = start_time.elapsed();
    state
        .stats_monitor
        .record_web_request("/stats", request_duration);

    info!(
        "Stats response generated for client: {} in {:.2}ms",
        addr,
        request_duration.as_secs_f64() * 1000.0
    );

    Json(stats_response)
}

/// Reset performance statistics
async fn stats_reset(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Json<serde_json::Value> {
    let start_time = Instant::now();
    info!("Stats reset endpoint called from client: {}", addr);

    let (prev_web_stats, prev_db_stats) = state.stats_monitor.get_and_reset_detailed_stats();

    let reset_response = serde_json::json!({
        "status": "success",
        "message": "Performance statistics have been reset",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "previous_stats": {
            "web_endpoints": prev_web_stats,
            "database_queries": prev_db_stats
        }
    });

    // Record stats reset endpoint performance
    let request_duration = start_time.elapsed();
    state
        .stats_monitor
        .record_web_request("/stats/reset", request_duration);

    info!(
        "Stats reset completed for client: {} in {:.2}ms",
        addr,
        request_duration.as_secs_f64() * 1000.0
    );

    Json(reset_response)
}

pub fn create_router(db_pool: Pool) -> Router {
    let state = AppState::new(db_pool);

    Router::new()
        .route("/health", get(health))
        .route("/hbd", get(hbd))
        .route("/stats", get(stats))
        .route("/stats/reset", get(stats_reset))
        .with_state(state)
}
