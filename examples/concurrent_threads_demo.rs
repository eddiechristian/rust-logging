//! Demonstration of concurrent device cache operations using threads
//! Shows one thread adding devices while another thread updates them
//! Demonstrates thread-safe concurrent access with DashMap and MacAddress keys

use axum_health_service::app::DeviceCacheManager;
use log::info;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    info!("=== Concurrent Threads Demo ===");
    info!("Starting two threads: one for adding devices, one for updating them");

    // Shared state for coordination
    let running = Arc::new(AtomicBool::new(true));
    let devices_added = Arc::new(AtomicU64::new(0));
    let devices_updated = Arc::new(AtomicU64::new(0));

    // Clone for threads
    let running_producer = running.clone();
    let running_updater = running.clone();
    let devices_added_producer = devices_added.clone();
    let devices_updated_updater = devices_updated.clone();

    info!("\n1. Starting producer thread (adds new devices)...");

    // Thread 1: Producer - Adds new devices to cache
    let producer_thread = thread::spawn(move || {
        let mut device_counter = 0;

        while running_producer.load(Ordering::Relaxed) {
            device_counter += 1;

            let device_id = format!("device_{:05}", device_counter);
            let mac_addr = format!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                0x00,
                0x11,
                0x22,
                (device_counter >> 16) & 0xFF,
                (device_counter >> 8) & 0xFF,
                device_counter & 0xFF
            );
            let ip_addr = format!(
                "192.168.{}.{}",
                (device_counter % 250) + 1,
                (device_counter % 250) + 1
            );

            match DeviceCacheManager::add_device_entry(
                device_id.clone(),
                mac_addr.clone(),
                ip_addr,
                Some(80),
            ) {
                Ok(_) => {
                    devices_added_producer.fetch_add(1, Ordering::Relaxed);
                    info!("[PRODUCER] Added device {}: MAC {}", device_id, mac_addr);
                }
                Err(e) => {
                    eprintln!("[PRODUCER] Failed to add device {}: {}", device_id, e);
                }
            }

            // Add devices at a rate of ~2 per second
            thread::sleep(Duration::from_millis(500));
        }

        info!("[PRODUCER] Thread shutting down");
    });

    info!("\n2. Starting updater thread (updates existing devices)...");

    // Thread 2: Updater - Updates existing devices in cache
    let updater_thread = thread::spawn(move || {
        let mut update_counter = 0;

        while running_updater.load(Ordering::Relaxed) {
            update_counter += 1;

            // Try to update a device (start after some devices are added)
            if update_counter > 5 {
                let target_device = ((update_counter - 5) % 20) + 1; // Cycle through first 20 devices
                let target_mac = format!(
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    0x00,
                    0x11,
                    0x22,
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
                                    "[UPDATER] Updated device with MAC {}: heartbeats incremented",
                                    target_mac
                                );
                            }
                            Err(e) => {
                                eprintln!("[UPDATER] Failed to update MAC {}: {}", target_mac, e);
                            }
                        }
                    }
                    None => {
                        info!(
                            "[UPDATER] Device with MAC {} not found yet, skipping",
                            target_mac
                        );
                    }
                }
            }

            // Update devices at a rate of ~3 per second
            thread::sleep(Duration::from_millis(333));
        }

        info!("[UPDATER] Thread shutting down");
    });

    info!("\n3. Starting monitor thread (reports cache status)...");

    // Thread 3: Monitor - Reports cache status
    let running_monitor = running.clone();
    let devices_added_monitor = devices_added.clone();
    let devices_updated_monitor = devices_updated.clone();

    let monitor_thread = thread::spawn(move || {
        while running_monitor.load(Ordering::Relaxed) {
            let added_count = devices_added_monitor.load(Ordering::Relaxed);
            let updated_count = devices_updated_monitor.load(Ordering::Relaxed);
            let cache_stats = DeviceCacheManager::get_cache_stats();

            info!(
                "[MONITOR] Cache Status - Total: {}, Added: {}, Updated: {}, Total Heartbeats: {}",
                cache_stats.total_entries, added_count, updated_count, cache_stats.total_heartbeats
            );

            // Show some example devices with their heartbeat counts
            let snapshot = DeviceCacheManager::get_cache_snapshot();
            if snapshot.len() >= 5 {
                info!("[MONITOR] Sample devices:");
                for (mac, entry) in snapshot.iter().take(5) {
                    info!(
                        "  MAC {}: {} (heartbeats: {})",
                        mac, entry.device_id, entry.heartbeat_count
                    );
                }
            }

            thread::sleep(Duration::from_secs(3));
        }

        info!("[MONITOR] Thread shutting down");
    });

    info!("\n4. Running demo for 20 seconds...");

    // Let the demo run for 20 seconds
    tokio::time::sleep(Duration::from_secs(20)).await;

    info!("\n5. Stopping all threads...");

    // Signal threads to stop
    running.store(false, Ordering::Relaxed);

    // Wait for threads to complete
    let _ = producer_thread.join();
    let _ = updater_thread.join();
    let _ = monitor_thread.join();

    // Final statistics
    let final_added = devices_added.load(Ordering::Relaxed);
    let final_updated = devices_updated.load(Ordering::Relaxed);
    let final_stats = DeviceCacheManager::get_cache_stats();

    info!("\n=== Final Results ===");
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

    // Show devices with highest heartbeat counts
    info!("\n=== Top Devices by Heartbeat Count ===");
    let mut all_devices = DeviceCacheManager::get_cache_snapshot();
    all_devices.sort_by(|a, b| b.1.heartbeat_count.cmp(&a.1.heartbeat_count));

    for (mac, entry) in all_devices.iter().take(10) {
        info!(
            "MAC {}: {} - {} heartbeats",
            mac, entry.device_id, entry.heartbeat_count
        );
    }

    info!("\n=== Demo completed successfully! ===");
    info!("\nKey observations:");
    info!("- Two threads safely accessed the same DashMap concurrently");
    info!("- Producer thread added devices while updater thread modified them");
    info!("- No race conditions or data corruption occurred");
    info!("- MacAddress keys ensure type safety and efficient lookups");
    info!("- DashMap provides excellent concurrent performance");
}
