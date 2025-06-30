//! Simple demonstration of device cache operations with DashMap using MacAddress keys
//! Shows both synchronous operations, async tasks, and full iteration capabilities

use axum_health_service::app::DeviceCacheManager;
use log::info;
use mac_address::MacAddress;
use std::str::FromStr;
use std::thread;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    // Initialize simple logging
    env_logger::init();

    info!("=== Device Cache Operations Demo ===");

    // Demonstrate basic cache operations
    info!("\n1. Adding devices to cache...");

    // Add some sample devices
    for i in 1..=5 {
        let device_id = format!("device_{:03}", i);
        let mac = format!("00:11:22:33:44:{:02}", i);
        let ip = format!("192.168.1.{}", 100 + i);

        if let Err(e) = DeviceCacheManager::add_device_entry(device_id.clone(), mac, ip, Some(80)) {
            eprintln!("Failed to add device {}: {}", device_id, e);
        }
    }

    info!("\n2. Retrieving devices from cache...");

    // Retrieve and display devices by MAC address
    for i in 1..=5 {
        let mac_str = format!("00:11:22:33:44:{:02}", i);

        if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac_str) {
            info!(
                "Retrieved MAC {}: device_id={}, IP={}, heartbeats={}, last_seen={}",
                mac_str, entry.device_id, entry.ip, entry.heartbeat_count, entry.last_seen
            );
        } else {
            info!("Device with MAC {} not found in cache", mac_str);
        }
    }

    info!("\n3. Updating device entries...");

    // Update some devices by MAC address
    for i in 1..=3 {
        let mac_str = format!("00:11:22:33:44:{:02}", i);

        if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac_str) {
            if let Err(e) = DeviceCacheManager::update_cache_entry_by_mac_str(&mac_str, entry) {
                eprintln!("Failed to update device with MAC {}: {}", mac_str, e);
            }
        }
    }

    info!("\n4. Starting async task demonstration...");

    // Start an async task that operates on the cache
    let async_task = tokio::spawn(async {
        for i in 0..3 {
            info!("[ASYNC TASK {}] Processing cache operations...", i + 1);

            // Add an async device
            let device_id = format!("async_device_{}", i + 1);
            let mac = format!("AA:BB:CC:DD:EE:{:02}", i);
            let ip = format!("10.0.0.{}", i + 10);

            if let Err(e) =
                DeviceCacheManager::add_device_entry(device_id.clone(), mac.clone(), ip, Some(443))
            {
                eprintln!("[ASYNC] Failed to add device {}: {}", device_id, e);
            }

            // Retrieve and update by MAC
            if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac) {
                info!(
                    "[ASYNC] Retrieved MAC {}: device_id={}, IP={}, heartbeats={}",
                    mac, entry.device_id, entry.ip, entry.heartbeat_count
                );

                if let Err(e) = DeviceCacheManager::update_cache_entry_by_mac_str(&mac, entry) {
                    eprintln!("[ASYNC] Failed to update device with MAC {}: {}", mac, e);
                }
            }

            sleep(Duration::from_secs(2)).await;
        }

        info!("[ASYNC TASK] Completed");
    });

    info!("\n5. Starting threaded cache operations...");

    // Start a thread that operates on the cache
    let thread_handle = thread::spawn(|| {
        for i in 0..3 {
            info!("[THREAD {}] Processing cache operations...", i + 1);

            // Add a threaded device
            let device_id = format!("thread_device_{}", i + 1);
            let mac = format!("FF:EE:DD:CC:BB:{:02}", i);
            let ip = format!("172.16.0.{}", i + 20);

            if let Err(e) =
                DeviceCacheManager::add_device_entry(device_id.clone(), mac.clone(), ip, Some(8080))
            {
                eprintln!("[THREAD] Failed to add device {}: {}", device_id, e);
            }

            // Retrieve and update by MAC
            if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac) {
                info!(
                    "[THREAD] Retrieved MAC {}: device_id={}, IP={}, heartbeats={}",
                    mac, entry.device_id, entry.ip, entry.heartbeat_count
                );

                if let Err(e) = DeviceCacheManager::update_cache_entry_by_mac_str(&mac, entry) {
                    eprintln!("[THREAD] Failed to update device with MAC {}: {}", mac, e);
                }
            }

            thread::sleep(Duration::from_secs(2));
        }

        info!("[THREAD] Completed");
    });

    info!("\n6. Waiting for background tasks to complete...");

    // Wait for both tasks to complete
    if let Err(e) = async_task.await {
        eprintln!("Async task failed: {}", e);
    }

    if let Err(e) = thread_handle.join() {
        eprintln!("Thread failed: {:?}", e);
    }

    info!("\n7. Full cache iteration demonstration...");

    // Demonstrate full cache iteration with DashMap
    let cache_snapshot = DeviceCacheManager::get_cache_snapshot();
    info!(
        "Complete cache snapshot contains {} entries:",
        cache_snapshot.len()
    );

    for (mac_address, entry) in cache_snapshot {
        info!(
            "Snapshot - MAC {}: device_id={}, IP={}, heartbeats={}, last_seen={}",
            mac_address, entry.device_id, entry.ip, entry.heartbeat_count, entry.last_seen
        );
    }

    info!("\n8. Cache statistics...");

    let stats = DeviceCacheManager::get_cache_stats();
    info!("Cache Statistics:");
    info!("  Total entries: {}", stats.total_entries);
    info!("  Active entries: {}", stats.active_entries);
    info!("  Stale entries: {}", stats.stale_entries);
    info!("  Total heartbeats: {}", stats.total_heartbeats);
    info!(
        "  Oldest entry age: {} seconds",
        stats.oldest_entry_age_seconds
    );
    info!(
        "  Newest entry age: {} seconds",
        stats.newest_entry_age_seconds
    );

    info!("\n9. Iterating with callback function...");

    DeviceCacheManager::iterate_cache_entries(|mac_address, entry| {
        let current_time = chrono::Utc::now().timestamp();
        let age = current_time - entry.last_seen;
        info!(
            "Callback iteration - MAC {}: device_id={}, {} seconds old, {} heartbeats",
            mac_address, entry.device_id, age, entry.heartbeat_count
        );
    });

    info!("\n10. Batch update operation...");

    let updated_count = DeviceCacheManager::update_all_entries(|mac_address, entry| {
        info!(
            "Batch updating MAC {}: device_id={}",
            mac_address, entry.device_id
        );
        entry.heartbeat_count += 10; // Add bonus heartbeats
        true // Keep all entries
    });

    info!("Batch updated {} entries", updated_count);

    info!("\n=== Demo completed successfully! ===");
    info!("\nKey takeaways:");
    info!("- DashMap provides excellent concurrent access from multiple threads and async tasks");
    info!("- No Guards or complex APIs required - simple and intuitive interface");
    info!("- Full iteration support with .iter(), .iter_mut(), and callback patterns");
    info!("- Built-in statistics and batch operations for cache management");
    info!("- Perfect for real-world concurrent caching scenarios");
}
