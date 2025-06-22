use anyhow::Result;
use chrono::{DateTime, Utc};
use lockfreehashmap::LockFreeHashMap;
use log::{error, info};
use mysql::prelude::Queryable;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::LazyLock;

use crate::server::AppState;

// Static lock-free hashmap for caching device data
static DEVICE_CACHE: LazyLock<LockFreeHashMap<String, DeviceCacheEntry>> = LazyLock::new(|| LockFreeHashMap::new());

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCacheEntry {
    pub mac: String,
    pub ip: String,
    pub last_ping: Option<i32>,
    pub last_seen: i64,
    pub heartbeat_count: u64,
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
            Ok(mut conn) => {
                let query_start = std::time::Instant::now();
                match conn.query_drop("SELECT 1") {
                    Ok(_) => {
                        let query_duration = query_start.elapsed();
                        let total_duration = start_time.elapsed();
                        
                        // Record database query performance for health check
                        state.stats_monitor.record_db_query("SELECT 1 (health_check)", query_duration);
                        
                        DatabaseHealth {
                            is_connected: true,
                            connection_test_duration_ms: Some(total_duration.as_millis() as u64),
                            error_message: None,
                        }
                    }
                    Err(e) => {
                        error!("Database health check query failed: {}", e);
                        
                        // Still record the failed query timing
                        let query_duration = query_start.elapsed();
                        state.stats_monitor.record_db_query("SELECT 1 (health_check_failed)", query_duration);
                        
                        DatabaseHealth {
                            is_connected: false,
                            connection_test_duration_ms: None,
                            error_message: Some(format!("Query failed: {}", e)),
                        }
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
    ) -> Result<HbdResponse> {
        info!(
            "Processing HBD for client {}: ID={}, MAC={}, IP={}",
            client_addr, params.id, params.mac, params.ip
        );

        // Validate input parameters
        Self::validate_hbd_params(&params)?;

        // Convert timestamp to ISO format if provided
        let timestamp_iso = Self::convert_timestamp_to_iso(params.ts)?;

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

        Ok(response)
    }

    /// Validate heartbeat parameters
    fn validate_hbd_params(params: &HbdParams) -> Result<()> {
        // Validate ID (should be positive)
        if params.id <= 0 {
            return Err(anyhow::anyhow!("Invalid ID: must be positive"));
        }

        // Validate MAC address format (basic check)
        if params.mac.is_empty() || params.mac.len() > 17 {
            return Err(anyhow::anyhow!("Invalid MAC address format"));
        }

        // Validate IP address format (basic check)
        if params.ip.is_empty() {
            return Err(anyhow::anyhow!("IP address cannot be empty"));
        }

        // Validate timestamp if provided
        if let Some(ts) = params.ts {
            if ts < 946684800 || ts > 4102444800 {
                return Err(anyhow::anyhow!(
                    "Invalid timestamp range: {} (must be between 2000-2100)",
                    ts
                ));
            }
        }

        Ok(())
    }

    /// Convert Unix timestamp to ISO format
    fn convert_timestamp_to_iso(timestamp: Option<i64>) -> Result<Option<String>> {
        match timestamp {
            Some(ts) => match DateTime::from_timestamp(ts, 0) {
                Some(dt) => Ok(Some(dt.to_rfc3339())),
                None => Err(anyhow::anyhow!(
                    "Failed to convert timestamp to ISO format: {}",
                    ts
                )),
            },
            None => Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_hbd_params_valid() {
        let params = HbdParams {
            id: 123,
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
            lp: Some(80),
            ts: Some(1609459200), // 2021-01-01
        };

        assert!(HbdService::validate_hbd_params(&params).is_ok());
    }

    #[test]
    fn test_validate_hbd_params_invalid_id() {
        let params = HbdParams {
            id: -1,
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
            lp: None,
            ts: None,
        };

        assert!(HbdService::validate_hbd_params(&params).is_err());
    }

    #[test]
    fn test_validate_hbd_params_invalid_timestamp() {
        let params = HbdParams {
            id: 123,
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
            lp: None,
            ts: Some(123), // Too old
        };

        assert!(HbdService::validate_hbd_params(&params).is_err());
    }

    #[test]
    fn test_convert_timestamp_to_iso() {
        let result = HbdService::convert_timestamp_to_iso(Some(1609459200));
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        let result = HbdService::convert_timestamp_to_iso(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
