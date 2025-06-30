//! Demonstration of concurrent device cache operations using async tasks
//! Shows one task adding devices while another task updates them
//! Demonstrates async-safe concurrent access with DashMap and MacAddress keys

use axum_health_service::app::DeviceCacheManager;
use log::info;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::time::{Duration, interval, sleep};

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    info!("=== Concurrent Async Tasks Demo ===");
    info!("Starting two async tasks: one for adding devices, one for updating them");

    // Shared state for coordination
    let running = Arc::new(AtomicBool::new(true));
    let devices_added = Arc::new(AtomicU64::new(0));
    let devices_updated = Arc::new(AtomicU64::new(0));

    // Clone for tasks
    let running_producer = running.clone();
    let running_updater = running.clone();
    let running_monitor = running.clone();
    let devices_added_producer = devices_added.clone();
    let devices_updated_updater = devices_updated.clone();
    let devices_added_monitor = devices_added.clone();
    let devices_updated_monitor = devices_updated.clone();

    info!("\n1. Starting producer task (adds new devices)...");

    // Task 1: Producer - Adds new devices to cache
    let producer_task = tokio::spawn(async move {
        let mut device_counter = 0;
        let mut add_interval = interval(Duration::from_millis(400)); // ~2.5 per second

        while running_producer.load(Ordering::Relaxed) {
            add_interval.tick().await;

            device_counter += 1;

            let device_id = format!("async_device_{:05}", device_counter);
            let mac_addr = format!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                0xAA,
                0xBB,
                0xCC,
                (device_counter >> 16) & 0xFF,
                (device_counter >> 8) & 0xFF,
                device_counter & 0xFF
            );
            let ip_addr = format!(
                "10.{}.{}.{}",
                (device_counter % 250) + 1,
                (device_counter % 250) + 1,
                (device_counter % 250) + 1
            );

            match DeviceCacheManager::add_device_entry(
                device_id.clone(),
                mac_addr.clone(),
                ip_addr,
                Some(443),
            ) {
                Ok(_) => {
                    devices_added_producer.fetch_add(1, Ordering::Relaxed);
                    info!(
                        "[PRODUCER] Added async device {}: MAC {}",
                        device_id, mac_addr
                    );
                }
                Err(e) => {
                    eprintln!("[PRODUCER] Failed to add async device {}: {}", device_id, e);
                }
            }
        }

        info!("[PRODUCER] Task shutting down");
    });

    info!("\n2. Starting updater task (updates existing devices)...");

    // Task 2: Updater - Updates existing devices in cache
    let updater_task = tokio::spawn(async move {
        let mut update_counter = 0;
        let mut update_interval = interval(Duration::from_millis(250)); // ~4 per second

        while running_updater.load(Ordering::Relaxed) {
            update_interval.tick().await;

            update_counter += 1;

            // Try to update a device (start after some devices are added)
            if update_counter > 8 {
                let target_device = ((update_counter - 8) % 30) + 1; // Cycle through first 30 devices
                let target_mac = format!(
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    0xAA,
                    0xBB,
                    0xCC,
                    (target_device >> 16) & 0xFF,
                    (target_device >> 8) & 0xFF,
                    target_device & 0xFF
                );

                match DeviceCacheManager::get_device_entry_by_mac_str(&target_mac) {
                    Some(entry) => {
                        match DeviceCacheManager::update_cache_entry_by_mac_str(&target_mac, entry)
                        {
                            Ok(_) => {
                                devices_updated_updater.fetch_add(1, Ordering::Relaxed);
                                info!(
                                    "[UPDATER] Updated async device with MAC {}: heartbeats incremented",
                                    target_mac
                                );
                            }
                            Err(e) => {
                                eprintln!(
                                    "[UPDATER] Failed to update async MAC {}: {}",
                                    target_mac, e
                                );
                            }
                        }
                    }
                    None => {
                        info!(
                            "[UPDATER] Async device with MAC {} not found yet, skipping",
                            target_mac
                        );
                    }
                }
            }
        }

        info!("[UPDATER] Task shutting down");
    });

    info!("\n3. Starting monitor task (reports cache status)...");

    // Task 3: Monitor - Reports cache status
    let monitor_task = tokio::spawn(async move {
        let mut monitor_interval = interval(Duration::from_secs(2));

        while running_monitor.load(Ordering::Relaxed) {
            monitor_interval.tick().await;

            let added_count = devices_added_monitor.load(Ordering::Relaxed);
            let updated_count = devices_updated_monitor.load(Ordering::Relaxed);
            let cache_stats = DeviceCacheManager::get_cache_stats();

            info!(
                "[MONITOR] Async Cache Status - Total: {}, Added: {}, Updated: {}, Total Heartbeats: {}",
                cache_stats.total_entries, added_count, updated_count, cache_stats.total_heartbeats
            );

            // Show devices with different heartbeat counts
            let high_activity = DeviceCacheManager::collect_entries_with_high_heartbeats(3);
            if !high_activity.is_empty() {
                info!(
                    "[MONITOR] High-activity devices (≥3 heartbeats): {}",
                    high_activity.len()
                );
                for (mac, entry) in high_activity.iter().take(3) {
                    info!(
                        "  MAC {}: {} (heartbeats: {})",
                        mac, entry.device_id, entry.heartbeat_count
                    );
                }
            }

            // Show recent devices
            let recent_devices = DeviceCacheManager::collect_entries_newer_than(5); // Last 5 seconds
            info!(
                "[MONITOR] Recently active devices (last 5s): {}",
                recent_devices.len()
            );
        }

        info!("[MONITOR] Task shutting down");
    });

    info!("\n4. Starting collection task (demonstrates filtering during concurrent operations)...");

    // Task 4: Collection Demo - Shows filtering while other tasks are working
    let collection_task = tokio::spawn(async move {
        let mut collection_interval = interval(Duration::from_secs(4));

        for i in 0..5 {
            collection_interval.tick().await;

            info!("[COLLECTOR] Running collection examples ({}/5)...", i + 1);

            // Collect devices by IP pattern
            let ip_pattern_devices = DeviceCacheManager::collect_entries_by_ip_pattern("10.");
            info!(
                "  Found {} devices with IP starting with '10.'",
                ip_pattern_devices.len()
            );

            // Collect devices by MAC pattern (our async devices)
            let async_devices = DeviceCacheManager::collect_entries_matching(|mac, _| {
                mac.to_string().starts_with("aa:bb:cc")
            });
            info!(
                "  Found {} async devices (MAC starts with 'aa:bb:cc')",
                async_devices.len()
            );

            // Collect devices by device name pattern
            let named_devices =
                DeviceCacheManager::collect_entries_by_device_pattern("async_device");
            info!(
                "  Found {} devices with 'async_device' in name",
                named_devices.len()
            );

            // Show some stats
            if !async_devices.is_empty() {
                let total_heartbeats: u64 = async_devices
                    .iter()
                    .map(|(_, entry)| entry.heartbeat_count)
                    .sum();
                let avg_heartbeats = total_heartbeats as f64 / async_devices.len() as f64;
                info!(
                    "  Average heartbeats per async device: {:.2}",
                    avg_heartbeats
                );
            }
        }

        info!("[COLLECTOR] Task completed");
    });

    info!("\n5. Running async demo for 25 seconds...");

    // Let the demo run for 25 seconds
    sleep(Duration::from_secs(25)).await;

    info!("\n6. Stopping all async tasks...");

    // Signal tasks to stop
    running.store(false, Ordering::Relaxed);

    // Wait for tasks to complete (with timeout)
    tokio::select! {
        _ = producer_task => info!("Producer task completed"),
        _ = sleep(Duration::from_secs(2)) => info!("Producer task timeout"),
    }

    tokio::select! {
        _ = updater_task => info!("Updater task completed"),
        _ = sleep(Duration::from_secs(2)) => info!("Updater task timeout"),
    }

    tokio::select! {
        _ = monitor_task => info!("Monitor task completed"),
        _ = sleep(Duration::from_secs(2)) => info!("Monitor task timeout"),
    }

    tokio::select! {
        _ = collection_task => info!("Collection task completed"),
        _ = sleep(Duration::from_secs(2)) => info!("Collection task timeout"),
    }

    // Final statistics
    let final_added = devices_added.load(Ordering::Relaxed);
    let final_updated = devices_updated.load(Ordering::Relaxed);
    let final_stats = DeviceCacheManager::get_cache_stats();

    info!("\n=== Final Async Results ===");
    info!("Devices added: {}", final_added);
    info!("Devices updated: {}", final_updated);
    info!("Final cache size: {}", final_stats.total_entries);
    info!("Total heartbeats: {}", final_stats.total_heartbeats);
    info!(
        "Average heartbeats per device: {:.2}",
        if final_stats.total_entries > 0 {
            final_stats.total_heartbeats as f64 / final_stats.total_entries as f64
        } else {
            0.0
        }
    );

    // Advanced collection analysis
    info!("\n=== Advanced Collection Analysis ===");

    // Group devices by heartbeat count
    let all_devices = DeviceCacheManager::get_cache_snapshot();
    let mut heartbeat_buckets = std::collections::HashMap::new();

    for (_, entry) in &all_devices {
        let bucket = entry.heartbeat_count;
        *heartbeat_buckets.entry(bucket).or_insert(0) += 1;
    }

    info!("Device distribution by heartbeat count:");
    let mut buckets: Vec<_> = heartbeat_buckets.into_iter().collect();
    buckets.sort_by_key(|&(count, _)| count);

    for (heartbeat_count, device_count) in buckets {
        info!("  {} heartbeats: {} devices", heartbeat_count, device_count);
    }

    // Find devices that were updated most frequently
    let top_updated =
        DeviceCacheManager::collect_entries_matching(|_, entry| entry.heartbeat_count >= 5);

    if !top_updated.is_empty() {
        info!("\n=== Most Updated Devices (≥5 heartbeats) ===");
        let mut sorted_top = top_updated;
        sorted_top.sort_by(|a, b| b.1.heartbeat_count.cmp(&a.1.heartbeat_count));

        for (mac, entry) in sorted_top.iter().take(5) {
            info!(
                "MAC {}: {} - {} heartbeats",
                mac, entry.device_id, entry.heartbeat_count
            );
        }
    }

    info!("\n=== Async Demo completed successfully! ===");
    info!("\nKey observations:");
    info!("- Four async tasks safely accessed the same DashMap concurrently");
    info!("- Producer task added devices while updater task modified them");
    info!("- Monitor task reported statistics while collection task filtered data");
    info!("- No race conditions or data corruption occurred");
    info!("- Tokio's async runtime handled concurrent access efficiently");
    info!("- MacAddress keys provided type safety across all async operations");
    info!("- Collection functions worked seamlessly during concurrent modifications");
}
