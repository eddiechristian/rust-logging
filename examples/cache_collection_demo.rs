//! Demonstration of collecting cache entries that match specific criteria
//! Shows the new collect_entries_matching function and related utilities

use axum_health_service::app::DeviceCacheManager;
use log::info;
use mac_address::MacAddress;
use std::str::FromStr;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    info!("=== Cache Collection Functions Demo ===");

    // Step 1: Populate cache with diverse test data
    populate_diverse_cache().await;

    // Step 2: Show initial cache state
    show_cache_state("Initial cache state");

    // Step 3: Demonstrate various collection functions
    demonstrate_collection_functions().await;

    info!("\n=== Demo completed ===");
}

async fn populate_diverse_cache() {
    info!("\n1. Populating cache with diverse test data...");

    // Add production servers
    let prod_devices = vec![
        ("prod_web_server_01", "00:50:56:12:34:01", "10.1.1.10"),
        ("prod_web_server_02", "00:50:56:12:34:02", "10.1.1.11"),
        ("prod_db_server_01", "00:50:56:78:90:01", "10.1.2.10"),
        ("prod_db_server_02", "00:50:56:78:90:02", "10.1.2.11"),
    ];

    // Add development devices
    let dev_devices = vec![
        ("dev_workstation_01", "AA:BB:CC:DD:EE:01", "192.168.100.10"),
        ("dev_workstation_02", "AA:BB:CC:DD:EE:02", "192.168.100.11"),
        ("dev_test_server", "AA:BB:CC:FF:FF:01", "192.168.100.50"),
    ];

    // Add IoT devices
    let iot_devices = vec![
        ("iot_sensor_temp_01", "DE:AD:BE:EF:01:01", "192.168.200.10"),
        ("iot_sensor_temp_02", "DE:AD:BE:EF:01:02", "192.168.200.11"),
        ("iot_camera_lobby", "DE:AD:BE:EF:02:01", "192.168.200.20"),
        ("iot_camera_parking", "DE:AD:BE:EF:02:02", "192.168.200.21"),
    ];

    // Add mobile devices
    let mobile_devices = vec![
        ("mobile_tablet_01", "11:22:33:44:55:01", "192.168.50.10"),
        ("mobile_phone_dev", "11:22:33:44:55:02", "192.168.50.11"),
    ];

    // Add all devices to cache
    let all_devices = [prod_devices, dev_devices, iot_devices, mobile_devices].concat();

    for (device_id, mac, ip) in all_devices {
        if let Err(e) = DeviceCacheManager::add_device_entry(
            device_id.to_string(),
            mac.to_string(),
            ip.to_string(),
            Some(80),
        ) {
            eprintln!("Failed to add device {}: {}", device_id, e);
        }
    }

    // Simulate some devices having more heartbeats
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Update production servers to have higher heartbeat counts
    for i in 1..=4 {
        let mac_patterns = ["00:50:56:12:34", "00:50:56:78:90"];
        for pattern in mac_patterns {
            let mac_str = format!("{}:{:02}", pattern, i.min(2));
            if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac_str) {
                for _ in 0..5 {
                    // Give them 5 extra heartbeats
                    let _ =
                        DeviceCacheManager::update_cache_entry_by_mac_str(&mac_str, entry.clone());
                }
            }
        }
    }

    info!("Added {} diverse test devices to cache", 13);
}

fn show_cache_state(title: &str) {
    info!("\n=== {} ===", title);

    let stats = DeviceCacheManager::get_cache_stats();
    info!("Cache size: {} entries", stats.total_entries);

    DeviceCacheManager::iterate_cache_entries(|mac_address, entry| {
        let current_time = chrono::Utc::now().timestamp();
        let age = current_time - entry.last_seen;
        info!(
            "  MAC {}: device_id={}, IP={}, heartbeats={}, age={}s",
            mac_address, entry.device_id, entry.ip, entry.heartbeat_count, age
        );
    });
}

async fn demonstrate_collection_functions() {
    info!("\n2. Demonstrating collection functions...");

    // Collection 1: Find all production devices
    info!("\n--- Collecting production devices ---");
    let prod_devices = DeviceCacheManager::collect_entries_by_device_pattern("prod_");
    info!("Found {} production devices:", prod_devices.len());
    for (mac, entry) in prod_devices {
        info!("  - MAC {}: {}", mac, entry.device_id);
    }

    // Collection 2: Find all devices on 192.168.100.x network
    info!("\n--- Collecting development network devices ---");
    let dev_network = DeviceCacheManager::collect_entries_by_ip_pattern("192.168.100.");
    info!(
        "Found {} devices on development network:",
        dev_network.len()
    );
    for (mac, entry) in dev_network {
        info!("  - MAC {}: {} at IP {}", mac, entry.device_id, entry.ip);
    }

    // Collection 3: Find high-activity devices
    info!("\n--- Collecting high-activity devices ---");
    let high_activity = DeviceCacheManager::collect_entries_with_high_heartbeats(3);
    info!(
        "Found {} high-activity devices (≥3 heartbeats):",
        high_activity.len()
    );
    for (mac, entry) in high_activity {
        info!(
            "  - MAC {}: {} with {} heartbeats",
            mac, entry.device_id, entry.heartbeat_count
        );
    }

    // Collection 4: Find recent devices
    info!("\n--- Collecting recently active devices ---");
    let recent_devices = DeviceCacheManager::collect_entries_newer_than(30); // Last 30 seconds
    info!("Found {} recently active devices:", recent_devices.len());
    for (mac, entry) in recent_devices {
        let age = chrono::Utc::now().timestamp() - entry.last_seen;
        info!(
            "  - MAC {}: {} (active {} seconds ago)",
            mac, entry.device_id, age
        );
    }

    // Collection 5: Custom criteria - IoT devices with specific MAC pattern
    info!("\n--- Collecting IoT devices with custom criteria ---");
    let iot_devices = DeviceCacheManager::collect_entries_matching(|mac, entry| {
        // IoT devices: have "iot" in device name AND MAC starts with "DE:AD:BE:EF"
        entry.device_id.contains("iot") && mac.to_string().starts_with("de:ad:be:ef")
    });
    info!("Found {} IoT devices matching criteria:", iot_devices.len());
    for (mac, entry) in iot_devices {
        info!("  - MAC {}: {} at IP {}", mac, entry.device_id, entry.ip);
    }

    // Collection 6: Complex custom criteria
    info!("\n--- Collecting with complex custom criteria ---");
    let complex_match = DeviceCacheManager::collect_entries_matching(|mac, entry| {
        // Complex criteria:
        // - Either production device with high heartbeats
        // - Or development device on specific network
        // - Or any device with very recent activity
        let current_time = chrono::Utc::now().timestamp();
        let age = current_time - entry.last_seen;

        (entry.device_id.contains("prod") && entry.heartbeat_count >= 5)
            || (entry.device_id.contains("dev") && entry.ip.starts_with("192.168.100"))
            || (age <= 5) // Very recent activity
    });
    info!(
        "Found {} devices matching complex criteria:",
        complex_match.len()
    );
    for (mac, entry) in complex_match {
        let age = chrono::Utc::now().timestamp() - entry.last_seen;
        info!(
            "  - MAC {}: {} (IP: {}, heartbeats: {}, age: {}s)",
            mac, entry.device_id, entry.ip, entry.heartbeat_count, age
        );
    }

    // Collection 7: Demonstrate filtering by MAC address characteristics
    info!("\n--- Collecting by MAC address patterns ---");

    // Collect devices with specific vendor prefix (00:50:56 is VMware)
    let vmware_devices = DeviceCacheManager::collect_entries_matching(|mac, _entry| {
        mac.to_string().starts_with("00:50:56")
    });
    info!(
        "Found {} VMware devices (MAC starts with 00:50:56):",
        vmware_devices.len()
    );
    for (mac, entry) in vmware_devices {
        info!("  - MAC {}: {}", mac, entry.device_id);
    }

    // Collection 8: Performance comparison
    info!("\n--- Performance comparison: Collection vs Iteration ---");

    let start_time = std::time::Instant::now();
    let collected = DeviceCacheManager::collect_entries_by_device_pattern("prod_");
    let collection_time = start_time.elapsed();

    let start_time = std::time::Instant::now();
    let mut iteration_count = 0;
    DeviceCacheManager::iterate_cache_entries(|_mac, entry| {
        if entry.device_id.contains("prod_") {
            iteration_count += 1;
        }
    });
    let iteration_time = start_time.elapsed();

    info!("Performance comparison:");
    info!(
        "  Collection function: found {} items in {:?}",
        collected.len(),
        collection_time
    );
    info!(
        "  Manual iteration: found {} items in {:?}",
        iteration_count, iteration_time
    );

    info!("\n--- Collection functions summary ---");
    info!("✓ collect_entries_matching() - Custom predicate function");
    info!("✓ collect_entries_by_device_pattern() - Device name pattern matching");
    info!("✓ collect_entries_by_ip_pattern() - IP address pattern matching");
    info!("✓ collect_entries_with_high_heartbeats() - Activity threshold filtering");
    info!("✓ collect_entries_newer_than() - Age-based filtering");
    info!("✓ All functions return Vec<(MacAddress, DeviceCacheEntry)> for easy processing");
}
