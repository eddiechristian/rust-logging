use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use log::{error, info};
use mac_address::MacAddress;
use mysql::prelude::Queryable;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;
use tokio::time::interval;

use crate::server::AppState;

// Static concurrent hashmap for caching device data with MacAddress as key
static DEVICE_CACHE: LazyLock<DashMap<MacAddress, DeviceCacheEntry>> =
    LazyLock::new(|| DashMap::new());

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCacheEntry {
    pub device_id: String, // Device identifier (moved from being the key)
    pub ip: String,
    pub last_ping: Option<i32>,
    pub last_seen: i64,
    pub heartbeat_count: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub stale_entries: usize,
    pub total_heartbeats: u64,
    pub oldest_entry_age_seconds: i64,
    pub newest_entry_age_seconds: i64,
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
                        state
                            .stats_monitor
                            .record_db_query("SELECT 1 (health_check)", query_duration);

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
                        state
                            .stats_monitor
                            .record_db_query("SELECT 1 (health_check_failed)", query_duration);

                        DatabaseHealth {
                            is_connected: false,
                            connection_test_duration_ms: None,
                            error_message: Some(format!("Query failed: {}", e)),
                        }
                    }
                }
            }
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

/// Cache management utilities for device cache operations
pub struct DeviceCacheManager;

impl DeviceCacheManager {
    /// Get a snapshot of all cache entries for iteration
    pub fn get_cache_snapshot() -> Vec<(MacAddress, DeviceCacheEntry)> {
        let mut entries = Vec::new();

        // DashMap provides excellent iteration support!
        for entry in DEVICE_CACHE.iter() {
            entries.push((*entry.key(), entry.value().clone()));
        }

        info!("Retrieved {} entries from device cache", entries.len());
        entries
    }

    /// Update a device cache entry by MAC address
    pub fn update_cache_entry(mac_address: MacAddress, mut entry: DeviceCacheEntry) -> Result<()> {
        entry.last_seen = Utc::now().timestamp();
        entry.heartbeat_count += 1;

        DEVICE_CACHE.insert(mac_address, entry);
        info!("Updated cache entry for MAC: {}", mac_address);
        Ok(())
    }

    /// Update a device cache entry by MAC address string
    pub fn update_cache_entry_by_mac_str(mac_str: &str, mut entry: DeviceCacheEntry) -> Result<()> {
        let mac_address = MacAddress::from_str(mac_str)
            .map_err(|e| anyhow::anyhow!("Invalid MAC address format '{}': {}", mac_str, e))?;

        entry.last_seen = Utc::now().timestamp();
        entry.heartbeat_count += 1;

        DEVICE_CACHE.insert(mac_address, entry);
        info!("Updated cache entry for MAC: {}", mac_address);
        Ok(())
    }

    /// Add a new device entry to cache
    pub fn add_device_entry(
        device_id: String,
        mac_str: String,
        ip: String,
        last_ping: Option<i32>,
    ) -> Result<()> {
        let mac_address = MacAddress::from_str(&mac_str)
            .map_err(|e| anyhow::anyhow!("Invalid MAC address format '{}': {}", mac_str, e))?;

        let entry = DeviceCacheEntry {
            device_id,
            ip,
            last_ping,
            last_seen: Utc::now().timestamp(),
            heartbeat_count: 1,
        };

        DEVICE_CACHE.insert(mac_address, entry);
        info!("Added new cache entry for MAC: {}", mac_address);
        Ok(())
    }

    /// Get a specific device entry from cache by MAC address
    pub fn get_device_entry_by_mac(mac_address: MacAddress) -> Option<DeviceCacheEntry> {
        DEVICE_CACHE.get(&mac_address).map(|entry| entry.clone())
    }

    /// Get a specific device entry from cache by MAC address string
    pub fn get_device_entry_by_mac_str(mac_str: &str) -> Option<DeviceCacheEntry> {
        if let Ok(mac_address) = MacAddress::from_str(mac_str) {
            DEVICE_CACHE.get(&mac_address).map(|entry| entry.clone())
        } else {
            None
        }
    }

    /// Remove a specific device entry from cache by MAC address
    pub fn remove_device_entry_by_mac(mac_address: MacAddress) -> Option<DeviceCacheEntry> {
        DEVICE_CACHE.remove(&mac_address).map(|(_, entry)| entry)
    }

    /// Remove a specific device entry from cache by MAC address string
    pub fn remove_device_entry_by_mac_str(mac_str: &str) -> Option<DeviceCacheEntry> {
        if let Ok(mac_address) = MacAddress::from_str(mac_str) {
            DEVICE_CACHE.remove(&mac_address).map(|(_, entry)| entry)
        } else {
            None
        }
    }

    /// Collect entries that match a given criteria (NEW FUNCTION)
    pub fn collect_entries_matching<F>(predicate: F) -> Vec<(MacAddress, DeviceCacheEntry)>
    where
        F: Fn(&MacAddress, &DeviceCacheEntry) -> bool,
    {
        let mut matching_entries = Vec::new();

        for entry in DEVICE_CACHE.iter() {
            if predicate(entry.key(), entry.value()) {
                matching_entries.push((*entry.key(), entry.value().clone()));
            }
        }

        info!(
            "Collected {} entries matching criteria",
            matching_entries.len()
        );
        matching_entries
    }

    /// Collect entries by device ID pattern
    pub fn collect_entries_by_device_pattern(
        device_pattern: &str,
    ) -> Vec<(MacAddress, DeviceCacheEntry)> {
        Self::collect_entries_matching(|_mac, entry| entry.device_id.contains(device_pattern))
    }

    /// Collect entries by IP pattern
    pub fn collect_entries_by_ip_pattern(ip_pattern: &str) -> Vec<(MacAddress, DeviceCacheEntry)> {
        Self::collect_entries_matching(|_mac, entry| entry.ip.contains(ip_pattern))
    }

    /// Collect entries with heartbeat count above threshold
    pub fn collect_entries_with_high_heartbeats(
        min_heartbeats: u64,
    ) -> Vec<(MacAddress, DeviceCacheEntry)> {
        Self::collect_entries_matching(|_mac, entry| entry.heartbeat_count >= min_heartbeats)
    }

    /// Collect entries newer than specified age
    pub fn collect_entries_newer_than(max_age_seconds: i64) -> Vec<(MacAddress, DeviceCacheEntry)> {
        let current_time = Utc::now().timestamp();
        Self::collect_entries_matching(|_mac, entry| {
            current_time - entry.last_seen <= max_age_seconds
        })
    }

    /// Iterate over all cache entries with a closure
    pub fn iterate_cache_entries<F>(mut callback: F)
    where
        F: FnMut(&MacAddress, &DeviceCacheEntry),
    {
        for entry in DEVICE_CACHE.iter() {
            callback(entry.key(), entry.value());
        }
    }

    /// Update all cache entries with a closure
    pub fn update_all_entries<F>(mut updater: F) -> usize
    where
        F: FnMut(&MacAddress, &mut DeviceCacheEntry) -> bool, // return true to keep, false to remove
    {
        let mut updated_count = 0;
        let mut to_remove = Vec::new();

        // First pass: update entries and collect keys to remove
        for mut entry in DEVICE_CACHE.iter_mut() {
            let key = *entry.key(); // Copy the MacAddress
            let should_keep = updater(&key, entry.value_mut());
            if !should_keep {
                to_remove.push(key);
            }
            updated_count += 1;
        }

        // Second pass: remove entries marked for deletion
        for key in to_remove {
            DEVICE_CACHE.remove(&key);
            info!("Removed cache entry for MAC: {}", key);
        }

        updated_count
    }

    /// Remove stale entries (older than specified duration in seconds)
    pub fn cleanup_stale_entries(max_age_seconds: i64) -> usize {
        let current_time = Utc::now().timestamp();
        let mut removed_count = 0;

        // Collect stale keys
        let stale_keys: Vec<MacAddress> = DEVICE_CACHE
            .iter()
            .filter(|entry| current_time - entry.value().last_seen > max_age_seconds)
            .map(|entry| *entry.key())
            .collect();

        // Remove stale entries
        for key in stale_keys {
            if DEVICE_CACHE.remove(&key).is_some() {
                removed_count += 1;
                info!("Removed stale cache entry for MAC: {}", key);
            }
        }

        info!("Cleaned up {} stale cache entries", removed_count);
        removed_count
    }

    /// Get current cache size
    pub fn get_cache_size() -> usize {
        DEVICE_CACHE.len()
    }

    /// Get cache statistics
    pub fn get_cache_stats() -> CacheStats {
        let current_time = Utc::now().timestamp();
        let mut stats = CacheStats {
            total_entries: 0,
            active_entries: 0,
            stale_entries: 0,
            total_heartbeats: 0,
            oldest_entry_age_seconds: 0,
            newest_entry_age_seconds: i64::MAX,
        };

        for entry in DEVICE_CACHE.iter() {
            stats.total_entries += 1;
            stats.total_heartbeats += entry.value().heartbeat_count;

            let age = current_time - entry.value().last_seen;

            if age > 300 {
                // 5 minutes
                stats.stale_entries += 1;
            } else {
                stats.active_entries += 1;
            }

            if age > stats.oldest_entry_age_seconds {
                stats.oldest_entry_age_seconds = age;
            }

            if age < stats.newest_entry_age_seconds {
                stats.newest_entry_age_seconds = age;
            }
        }

        if stats.total_entries == 0 {
            stats.newest_entry_age_seconds = 0;
        }

        stats
    }

    /// Start a background thread for cache maintenance
    pub fn start_cache_maintenance_thread(
        cleanup_interval_seconds: u64,
        max_age_seconds: i64,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            info!(
                "Starting device cache maintenance thread with {}s interval",
                cleanup_interval_seconds
            );

            loop {
                thread::sleep(Duration::from_secs(cleanup_interval_seconds));

                let removed_count = Self::cleanup_stale_entries(max_age_seconds);
                let cache_size = Self::get_cache_size();
                info!(
                    "Cache maintenance completed. Current size: {}, Removed: {}",
                    cache_size, removed_count
                );
            }
        })
    }

    /// Start async cache maintenance task
    pub async fn start_cache_maintenance_async(
        cleanup_interval_seconds: u64,
        max_age_seconds: i64,
    ) {
        info!(
            "Starting async device cache maintenance with {}s interval",
            cleanup_interval_seconds
        );

        let mut interval_timer = interval(Duration::from_secs(cleanup_interval_seconds));

        loop {
            interval_timer.tick().await;

            let removed_count = Self::cleanup_stale_entries(max_age_seconds);
            let cache_size = Self::get_cache_size();
            info!(
                "Async cache maintenance completed. Current size: {}, Removed: {}",
                cache_size, removed_count
            );
        }
    }

    /// Remove cache entries that match a given predicate
    pub fn remove_entries_matching_mac<F>(predicate: F) -> usize
    where
        F: Fn(&MacAddress, &DeviceCacheEntry) -> bool,
    {
        let mut removed_count = 0;

        // Collect keys that match the predicate
        let keys_to_remove: Vec<MacAddress> = DEVICE_CACHE
            .iter()
            .filter(|entry| predicate(entry.key(), entry.value()))
            .map(|entry| *entry.key())
            .collect();

        // Remove the matching entries
        for key in keys_to_remove {
            if DEVICE_CACHE.remove(&key).is_some() {
                removed_count += 1;
                info!("Removed cache entry for MAC: {} (matched criteria)", key);
            }
        }

        info!("Removed {} cache entries matching criteria", removed_count);
        removed_count
    }

    /// Remove cache entries by IP address pattern
    pub fn remove_entries_by_ip_pattern(ip_pattern: &str) -> usize {
        Self::remove_entries_matching_mac(|_mac, entry| entry.ip.contains(ip_pattern))
    }

    /// Remove cache entries by MAC address pattern (as string)
    pub fn remove_entries_by_mac_pattern(mac_pattern: &str) -> usize {
        Self::remove_entries_matching_mac(|mac, _entry| mac.to_string().contains(mac_pattern))
    }

    /// Remove cache entries with low heartbeat count
    pub fn remove_entries_with_low_heartbeats(min_heartbeats: u64) -> usize {
        Self::remove_entries_matching_mac(|_mac, entry| entry.heartbeat_count < min_heartbeats)
    }

    /// Remove cache entries by device ID pattern
    pub fn remove_entries_by_device_pattern(device_pattern: &str) -> usize {
        Self::remove_entries_matching_mac(|_mac, entry| entry.device_id.contains(device_pattern))
    }

    /// Remove cache entries older than specified age (more flexible than cleanup_stale_entries)
    pub fn remove_entries_older_than(max_age_seconds: i64) -> usize {
        let current_time = Utc::now().timestamp();
        Self::remove_entries_matching_mac(|_mac, entry| {
            current_time - entry.last_seen > max_age_seconds
        })
    }

    /// Iterate and conditionally remove entries with detailed logging
    pub fn iterate_and_remove_with_logging<F>(mut condition: F) -> (usize, usize)
    where
        F: FnMut(&MacAddress, &DeviceCacheEntry) -> bool,
    {
        let mut total_checked = 0;
        let mut removed_count = 0;
        let mut keys_to_remove = Vec::new();

        // First pass: iterate and check conditions
        for entry in DEVICE_CACHE.iter() {
            total_checked += 1;
            let mac_address = entry.key();
            let cache_entry = entry.value();

            info!(
                "Checking MAC {}: device_id={}, IP={}, heartbeats={}, age={}s",
                mac_address,
                cache_entry.device_id,
                cache_entry.ip,
                cache_entry.heartbeat_count,
                Utc::now().timestamp() - cache_entry.last_seen
            );

            if condition(mac_address, cache_entry) {
                info!("MAC {} marked for removal", mac_address);
                keys_to_remove.push(*mac_address);
            }
        }

        // Second pass: remove marked entries
        for key in keys_to_remove {
            if let Some((_, removed_entry)) = DEVICE_CACHE.remove(&key) {
                removed_count += 1;
                info!(
                    "Removed MAC {}: device_id={}, IP={}, heartbeats={}",
                    key, removed_entry.device_id, removed_entry.ip, removed_entry.heartbeat_count
                );
            }
        }

        info!(
            "Iteration completed: checked {} entries, removed {} entries",
            total_checked, removed_count
        );

        (total_checked, removed_count)
    }

    /// Advanced removal with multiple criteria
    pub fn remove_entries_advanced_criteria(
        max_age_seconds: Option<i64>,
        min_heartbeats: Option<u64>,
        ip_patterns: Option<&[&str]>,
        mac_patterns: Option<&[&str]>,
        device_patterns: Option<&[&str]>,
    ) -> usize {
        let current_time = Utc::now().timestamp();

        Self::remove_entries_matching_mac(|mac_address, entry| {
            // Check age criteria
            if let Some(max_age) = max_age_seconds {
                if current_time - entry.last_seen > max_age {
                    return true;
                }
            }

            // Check heartbeat criteria
            if let Some(min_beats) = min_heartbeats {
                if entry.heartbeat_count < min_beats {
                    return true;
                }
            }

            // Check IP patterns
            if let Some(patterns) = ip_patterns {
                for pattern in patterns {
                    if entry.ip.contains(pattern) {
                        return true;
                    }
                }
            }

            // Check MAC patterns
            if let Some(patterns) = mac_patterns {
                let mac_str = mac_address.to_string();
                for pattern in patterns {
                    if mac_str.contains(pattern) {
                        return true;
                    }
                }
            }

            // Check device ID patterns
            if let Some(patterns) = device_patterns {
                for pattern in patterns {
                    if entry.device_id.contains(pattern) {
                        return true;
                    }
                }
            }

            false
        })
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
