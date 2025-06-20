use anyhow::Result;
use chrono::{DateTime, Utc};
use lockfreehashmap::LockFreeHashMap;
use log::{error, info};
use mysql::prelude::Queryable;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::server::AppState;
use axum::{
    http::StatusCode,
    response::Json,
};

// Static lock-free hashmap for caching device data
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCacheEntry {
    pub id: u64,
    pub mac: String,
    pub ip: String,
    pub pip: String,
    pub long_poll: u8,
    pub last_hb_cache_write: Option<DateTime<Utc>>,
}

static DEVICE_CACHE: std::sync::LazyLock<LockFreeHashMap<String, DeviceCacheEntry>> =
    std::sync::LazyLock::new(|| LockFreeHashMap::new());


struct AuthorizedResult{
    authorized: bool,
    squelched: bool,
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
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    pub service_name: String,
    pub version: String,
    pub health_count: u64,
    pub user_agent: Option<String>,
    pub headers_count: usize,
    pub database_status: String,
}

#[derive(Serialize)]
pub struct DatabaseHealth {
    pub is_connected: bool,
    pub connection_test_duration_ms: Option<u64>,
    pub error_message: Option<String>,
}

/// Business logic for health check endpoint
pub struct HealthService;

impl HealthService {
    /// Process health check request and return response
    pub fn process_health_check(
        state: &AppState,
        client_addr: SocketAddr,
        headers_count: usize,
        user_agent: Option<String>,
    ) -> HealthResponse {
        info!("Processing health check for client: {}", client_addr);

        // Increment health counter
        let current_count = state.health_count.fetch_add(1) + 1;

        // Check database health
        let db_health = Self::check_database_health(state);

        let overall_status = if db_health.is_connected {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };

        let database_status = if db_health.is_connected {
            "connected".to_string()
        } else {
            "disconnected".to_string()
        };

        let response = HealthResponse {
            status: overall_status,
            timestamp: Utc::now().to_rfc3339(),
            service_name: state.service_name.clone(),
            version: state.version.clone(),
            health_count: current_count,
            user_agent,
            headers_count,
            database_status,
        };

        info!(
            "Health check completed for client {}: status={}, db_connected={}, count={}",
            client_addr, response.status, db_health.is_connected, current_count
        );

        response
    }

    /// Check database connectivity and performance
    pub fn check_database_health(state: &AppState) -> DatabaseHealth {
        let start_time = std::time::Instant::now();

        match state.get_connection() {
            Ok(mut conn) => match conn.query_drop("SELECT 1") {
                Ok(_) => {
                    let duration = start_time.elapsed();
                    DatabaseHealth {
                        is_connected: true,
                        connection_test_duration_ms: Some(duration.as_millis() as u64),
                        error_message: None,
                    }
                }
                Err(e) => {
                    error!("Database health check query failed: {}", e);
                    DatabaseHealth {
                        is_connected: false,
                        connection_test_duration_ms: None,
                        error_message: Some(format!("Query failed: {}", e)),
                    }
                }
            },
            Err(e) => {
                error!("Failed to get database connection for health check: {}", e);
                DatabaseHealth {
                    is_connected: false,
                    connection_test_duration_ms: None,
                    error_message: Some(format!("Connection failed: {}", e)),
                }
            }
        }
    }
}

/// Business logic for heartbeat (HBD) endpoint
pub struct HbdService;

impl HbdService {
    /// Process heartbeat data and return response
    pub fn process_heartbeat(
        state: &AppState,
        params: HbdParams,
        client_addr: SocketAddr,
    ) -> Result<Json<crate::app::HbdResponse>, StatusCode> {
        info!(
            "Processing HBD for client {}: ID={}, MAC={}, IP={}",
            client_addr, params.id, params.mac, params.ip
        );

        // Convert timestamp to ISO format if provided
        let timestamp_iso = Self::convert_timestamp_to_iso(params.ts);

        // Increment HBD counter
        let current_count = state.hbd_count.fetch_add(1) + 1;

        // Here you could add database persistence logic
        // Self::persist_heartbeat_data(&state, &params)?;

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
            processed_at: Utc::now().to_rfc3339(),
        };

        info!(
            "HBD processed successfully for client {}: count={}",
            client_addr, current_count
        );

        Ok(Json(response))
    }

    /// is mac in cache or db
    fn get_authorized(
        &self, 
        state: &AppState,
        mac: &str,
    ) -> Result<AuthorizedResult, StatusCode> {
         let guard = lockfreehashmap::pin();
         match DEVICE_CACHE.get(mac, &guard) {
            None => {
                //call db to get auth and squelched.
                self.call_is_device_active(state, mac)
            }
            Some(_) => Ok(AuthorizedResult {
                authorized: true,
                squelched: false,
            }),
        }
    }

    fn call_is_device_active(
        &self,
        state: &AppState,
        mac: &str,
    ) -> Result<AuthorizedResult, StatusCode> {
        // Call the stored procedure
        match state.get_connection() {
            Ok(mut conn) => {
                let result: Result<Vec<mysql::Row>, mysql::Error> =
                    conn.exec("CALL is_device_active(?, @msg)", (mac,));

                // Handle the @msg output parameter properly
                let _message: Result<Option<String>, mysql::Error> =
                    conn.query_first("SELECT @msg");

                match result {
                    Ok(mut rows) => {
                        if let Some(row) = rows.pop() {
                            let (_account_id, squelch): (Option<i32>, i32) = mysql::from_row(row);
                            Ok(AuthorizedResult {
                                authorized: true,
                                squelched: squelch != 0,
                            })
                        } else {
                            Ok(AuthorizedResult {
                                authorized: false,
                                squelched: true,
                            })
                        }
                    }
                    Err(_) => Ok(AuthorizedResult {
                        authorized: false,
                        squelched: true,
                    }),
                }
            }
            Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
        }
    }


    /// Get Last database presist for this cache entry.
    fn get_last_heartbeat_write(mac: &str) -> Option<DateTime<Utc>> {
        let guard = lockfreehashmap::pin();
        match DEVICE_CACHE.get(mac, &guard) {
            None => None,
            Some(cached_device) => cached_device.last_hb_cache_write,
        }
    }


    /// Convert Unix timestamp to ISO format
    fn convert_timestamp_to_iso(timestamp: Option<i64>) ->Option<String> {
        match timestamp {
            Some(ts) => match DateTime::from_timestamp(ts, 0) {
                Some(dt) => Some(dt.to_rfc3339()),
                None => None
            },
            None => None,
        }
    }

    /// Persist heartbeat data to database (placeholder for future implementation)
    #[allow(dead_code)]
    fn persist_heartbeat_data(_state: &AppState, params: &HbdParams) -> Result<()> {
        // This is where you would implement database persistence
        // Example:
        // let mut conn = state.get_connection()?;
        // conn.exec_drop(
        //     "INSERT INTO heartbeats (device_id, mac_address, ip_address, last_ping, timestamp) VALUES (?, ?, ?, ?, ?)",
        //     (params.id, &params.mac, &params.ip, params.lp, params.ts)
        // )?;

        info!(
            "Heartbeat data would be persisted for device ID: {}",
            params.id
        );
        Ok(())
    }
}
