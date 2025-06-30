//! Example demonstrating device cache operations with DashMap
//! Shows both threading and async approaches for cache management with full iteration support
//!
//! DashMap provides excellent iteration capabilities while maintaining high performance

use axum_health_service::app::{DeviceCacheEntry, DeviceCacheManager};
use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::time::interval;

// Example of full cache iteration in a separate thread
fn manage_cache_in_thread() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        info!("Starting cache management thread with full iteration");

        // Sample device IDs to work with
        let device_ids = vec!["device_001", "device_002", "device_003"];

        loop {
            info!("Processing cache operations in thread");

            // Example: Add some sample devices
            for (i, device_id) in device_ids.iter().enumerate() {
                if let Err(e) = DeviceCacheManager::add_device_entry(
                    device_id.to_string(),
                    format!("00:11:22:33:44:{:02}", i),
                    format!("192.168.1.{}", 100 + i),
                    Some(80),
                ) {
                    eprintln!("Failed to add device {}: {}", device_id, e);
                }
            }

            // Example: Iterate over ALL cache entries (this is what DashMap excels at!)
            info!("[THREAD] Iterating over all cache entries:");
            DeviceCacheManager::iterate_cache_entries(|mac_address, entry| {
                let current_time = chrono::Utc::now().timestamp();
                let time_since_last_seen = current_time - entry.last_seen;

                info!(
                    "[THREAD] MAC {}: device_id={}, IP={}, last seen {} seconds ago, heartbeats={}",
                    mac_address,
                    entry.device_id,
                    entry.ip,
                    time_since_last_seen,
                    entry.heartbeat_count
                );
            });

            // Example: Batch update all entries
            let updated_count = DeviceCacheManager::update_all_entries(|mac_address, entry| {
                info!(
                    "[THREAD] Batch updating MAC {}: device_id={}",
                    mac_address, entry.device_id
                );
                entry.heartbeat_count += 1; // Increment heartbeat
                true // Keep all entries
            });

            info!("[THREAD] Batch updated {} entries", updated_count);

            // Sleep for 30 seconds before next iteration
            thread::sleep(Duration::from_secs(30));
        }
    })
}

// Example of async cache management
async fn manage_cache_async() {
    info!("Starting async cache management");

    let mut interval_timer = interval(Duration::from_secs(30));

    // Sample device IDs to work with
    let device_ids = vec!["async_device_001", "async_device_002", "async_device_003"];

    loop {
        interval_timer.tick().await;

        info!("Processing async cache operations");

        // Example: Add some sample devices
        for (i, device_id) in device_ids.iter().enumerate() {
            if let Err(e) = DeviceCacheManager::add_device_entry(
                device_id.to_string(),
                format!("AA:BB:CC:DD:EE:{:02}", i),
                format!("10.0.0.{}", 10 + i),
                Some(443),
            ) {
                eprintln!("Failed to add async device {}: {}", device_id, e);
            }
        }

        // Example: Process specific devices by MAC
        let mac_addresses = vec![
            "AA:BB:CC:DD:EE:01",
            "AA:BB:CC:DD:EE:02",
            "AA:BB:CC:DD:EE:03",
        ];
        for mac_str in &mac_addresses {
            if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(mac_str) {
                let current_time = chrono::Utc::now().timestamp();
                let time_since_last_seen = current_time - entry.last_seen;

                info!(
                    "[ASYNC] MAC {}: device_id={}, IP={}, last seen {} seconds ago, heartbeats={}",
                    mac_str, entry.device_id, entry.ip, time_since_last_seen, entry.heartbeat_count
                );

                // Update the entry
                if let Err(e) = DeviceCacheManager::update_cache_entry_by_mac_str(mac_str, entry) {
                    eprintln!(
                        "Failed to update async cache entry for MAC {}: {}",
                        mac_str, e
                    );
                }
            }
        }
    }
}

// Example of async device health monitoring with specific device tracking
async fn monitor_device_health_async() {
    info!("Starting async device health monitoring");

    let mut interval_timer = interval(Duration::from_secs(60)); // Check every minute

    // Keep track of known devices (in real scenario, this could come from database)
    let known_devices = vec![
        "device_001",
        "device_002",
        "device_003",
        "async_device_001",
        "async_device_002",
        "async_device_003",
    ];

    loop {
        interval_timer.tick().await;

        let current_time = chrono::Utc::now().timestamp();
        let mut unhealthy_devices = 0;
        let mut stale_devices = 0;
        let mut active_devices = 0;

        // Check known MAC addresses
        let known_macs = vec![
            "00:11:22:33:44:01",
            "00:11:22:33:44:02",
            "00:11:22:33:44:03",
            "AA:BB:CC:DD:EE:01",
            "AA:BB:CC:DD:EE:02",
            "AA:BB:CC:DD:EE:03",
        ];

        for mac_str in &known_macs {
            if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(mac_str) {
                let time_since_last_seen = current_time - entry.last_seen;

                if time_since_last_seen > 600 {
                    // 10 minutes
                    stale_devices += 1;
                    info!(
                        "Stale device detected: MAC {} (last seen {} seconds ago)",
                        mac_str, time_since_last_seen
                    );
                } else if time_since_last_seen > 120 {
                    // 2 minutes
                    unhealthy_devices += 1;
                    info!(
                        "Potentially unhealthy device: MAC {} (last seen {} seconds ago)",
                        mac_str, time_since_last_seen
                    );
                } else {
                    active_devices += 1;
                }
            } else {
                info!("Device with MAC {} not found in cache", mac_str);
            }
        }

        info!(
            "Health check completed. Active: {}, Unhealthy: {}, Stale: {}",
            active_devices, unhealthy_devices, stale_devices
        );
    }
}

// Alternative approach using a traditional HashMap with mutex for full iteration
struct AlternativeCacheManager {
    cache: Arc<Mutex<HashMap<String, DeviceCacheEntry>>>,
}

impl AlternativeCacheManager {
    fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn add_device(&self, id: String, entry: DeviceCacheEntry) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(id.clone(), entry);
            info!("Added device {} to alternative cache", id);
        }
    }

    fn iterate_all_devices(&self) -> Vec<(String, DeviceCacheEntry)> {
        if let Ok(cache) = self.cache.lock() {
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            Vec::new()
        }
    }
}

// Example using alternative cache that supports full iteration
async fn demonstrate_alternative_cache() {
    info!("Demonstrating alternative cache approach with full iteration");

    let alt_cache = AlternativeCacheManager::new();

    // Add some sample devices
    for i in 0..5 {
        let entry = DeviceCacheEntry {
            device_id: format!("alt_device_{:03}", i),
            ip: format!("172.16.0.{}", i + 1),
            last_ping: Some(80),
            last_seen: chrono::Utc::now().timestamp(),
            heartbeat_count: i as u64,
        };
        alt_cache.add_device(format!("alt_device_{:03}", i), entry);
    }

    // Iterate over all devices
    let all_devices = alt_cache.iterate_all_devices();
    info!("Found {} devices in alternative cache", all_devices.len());

    for (device_id, entry) in all_devices {
        info!(
            "Alternative cache - Device {}: IP={}, heartbeats={}",
            device_id, entry.ip, entry.heartbeat_count
        );
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    info!("Starting cache iteration examples");

    // Example 1: Start cache maintenance in a separate thread
    let _maintenance_thread = DeviceCacheManager::start_cache_maintenance_thread(
        300,  // Clean every 5 minutes
        1800, // Remove entries older than 30 minutes
    );

    // Example 2: Start cache management in a separate thread
    let _iteration_thread = manage_cache_in_thread();

    // Example 3: Start async cache maintenance
    let maintenance_task = tokio::spawn(async {
        DeviceCacheManager::start_cache_maintenance_async(
            300,  // Clean every 5 minutes
            1800, // Remove entries older than 30 minutes
        )
        .await;
    });

    // Example 4: Start async cache management
    let iteration_task = tokio::spawn(manage_cache_async());

    // Example 5a: Alternative cache demonstration
    let alt_cache_task = tokio::spawn(demonstrate_alternative_cache());

    // Example 5b: Start device health monitoring
    let health_task = tokio::spawn(monitor_device_health_async());

    info!("All tasks started. Press Ctrl+C to exit.");

    // Wait for tasks (they run indefinitely)
    tokio::select! {
        _ = maintenance_task => {},
        _ = iteration_task => {},
        _ = alt_cache_task => {},
        _ = health_task => {},
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
    }

    info!("Application shutting down");
}
