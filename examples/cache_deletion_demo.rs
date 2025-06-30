//! Comprehensive demonstration of iterating and deleting cache entries based on criteria
//! Shows various patterns for conditional cache cleanup using DashMap

use axum_health_service::app::DeviceCacheManager;
use log::info;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    info!("=== Cache Deletion Criteria Demo ===");

    // Step 1: Populate cache with test data
    populate_test_cache();

    // Step 2: Show initial cache state
    show_cache_state("Initial cache state");

    // Step 3: Demonstrate different deletion criteria
    demonstrate_deletion_patterns().await;

    info!("\n=== Demo completed ===");
}

fn populate_test_cache() {
    info!("\n1. Populating cache with test data...");

    // Add devices with different characteristics for testing
    let test_devices = vec![
        ("prod_server_001", "00:11:22:33:44:01", "192.168.1.10"),
        ("prod_server_002", "00:11:22:33:44:02", "192.168.1.11"),
        ("test_device_001", "AA:BB:CC:DD:EE:01", "10.0.0.100"),
        ("test_device_002", "AA:BB:CC:DD:EE:02", "10.0.0.101"),
        ("dev_workstation_001", "FF:EE:DD:CC:BB:01", "172.16.0.50"),
        ("dev_workstation_002", "FF:EE:DD:CC:BB:02", "172.16.0.51"),
        ("legacy_system_001", "11:22:33:44:55:01", "203.0.113.10"),
        ("legacy_system_002", "11:22:33:44:55:02", "203.0.113.11"),
        ("mobile_device_001", "77:88:99:AA:BB:01", "192.168.2.100"),
        ("mobile_device_002", "77:88:99:AA:BB:02", "192.168.2.101"),
    ];

    for (device_id, mac, ip) in test_devices {
        if let Err(e) = DeviceCacheManager::add_device_entry(
            device_id.to_string(),
            mac.to_string(),
            ip.to_string(),
            Some(80),
        ) {
            eprintln!("Failed to add device {}: {}", device_id, e);
        }
    }

    // Simulate some devices with more heartbeats
    thread::sleep(Duration::from_millis(100));

    for i in 0..5 {
        let mac_str = format!("00:11:22:33:44:{:02}", i + 1);
        if let Some(entry) = DeviceCacheManager::get_device_entry_by_mac_str(&mac_str) {
            let _ = DeviceCacheManager::update_cache_entry_by_mac_str(&mac_str, entry);
        }
    }

    info!("Added {} test devices to cache", 10);
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

async fn demonstrate_deletion_patterns() {
    // Pattern 1: Remove by IP pattern
    info!("\n2. Removing devices with IP pattern '10.0.0.'...");
    let removed = DeviceCacheManager::remove_entries_by_ip_pattern("10.0.0.");
    info!("Removed {} devices with IP pattern '10.0.0.'", removed);
    show_cache_state("After IP pattern removal");

    // Wait a bit to simulate time passing
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Pattern 2: Remove by MAC pattern
    info!("\n3. Removing devices with MAC pattern 'FF:EE:DD'...");
    let removed = DeviceCacheManager::remove_entries_by_mac_pattern("FF:EE:DD");
    info!("Removed {} devices with MAC pattern 'FF:EE:DD'", removed);
    show_cache_state("After MAC pattern removal");

    // Pattern 3: Remove by device name pattern
    info!("\n4. Removing devices with name pattern 'legacy'...");
    let removed = DeviceCacheManager::remove_entries_by_device_pattern("legacy");
    info!("Removed {} devices with name pattern 'legacy'", removed);
    show_cache_state("After device name pattern removal");

    // Pattern 4: Remove by low heartbeat count
    info!("\n5. Removing devices with less than 3 heartbeats...");
    let removed = DeviceCacheManager::remove_entries_with_low_heartbeats(3);
    info!("Removed {} devices with low heartbeats", removed);
    show_cache_state("After low heartbeat removal");

    // Pattern 5: Custom condition with detailed logging
    info!("\n6. Using custom removal condition with detailed logging...");
    let (checked, removed) =
        DeviceCacheManager::iterate_and_remove_with_logging(|mac_address, entry| {
            // Remove mobile devices with specific MAC patterns
            entry.device_id.contains("mobile") && mac_address.to_string().contains("77:88")
        });
    info!(
        "Custom removal: checked {} entries, removed {} entries",
        checked, removed
    );
    show_cache_state("After custom condition removal");

    // Pattern 6: Advanced criteria (multiple conditions)
    info!("\n7. Using advanced criteria removal...");

    // Add more test devices for advanced criteria demo
    for i in 0..3 {
        let device_id = format!("temp_device_{:03}", i);
        let _ = DeviceCacheManager::add_device_entry(
            device_id,
            format!("99:99:99:99:99:{:02}", i),
            format!("192.168.99.{}", i + 1),
            Some(443),
        );
    }

    show_cache_state("After adding temp devices");

    // Remove using advanced criteria
    let removed = DeviceCacheManager::remove_entries_advanced_criteria(
        None,                               // No age limit
        Some(2),                            // Less than 2 heartbeats
        Some(&["192.168.99", "192.168.2"]), // IP patterns
        Some(&["99:99:99"]),                // MAC patterns
        Some(&["temp", "mobile"]),          // Device name patterns
    );
    info!("Advanced criteria removed {} devices", removed);
    show_cache_state("After advanced criteria removal");

    // Pattern 7: Remove entries older than specific age
    info!("\n8. Simulating time passage and age-based removal...");

    // Add a device that will be "old"
    let _ = DeviceCacheManager::add_device_entry(
        "old_device_001".to_string(),
        "AA:AA:AA:AA:AA:01".to_string(),
        "172.16.99.1".to_string(),
        Some(22),
    );

    // Sleep to make it "older"
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Remove entries older than 2 seconds
    let removed = DeviceCacheManager::remove_entries_older_than(2);
    info!("Removed {} entries older than 2 seconds", removed);
    show_cache_state("After age-based removal");

    // Pattern 8: Custom complex condition
    info!("\n9. Complex custom removal condition...");

    // Add some final test devices
    for i in 0..3 {
        let device_id = format!("final_test_{:03}", i);
        let _ = DeviceCacheManager::add_device_entry(
            device_id,
            format!("BB:BB:BB:BB:BB:{:02}", i),
            format!("10.10.10.{}", i + 1),
            Some(8080),
        );
    }

    let removed = DeviceCacheManager::remove_entries_matching_mac(|mac_address, entry| {
        // Complex condition: remove if device name contains "final"
        // AND (IP starts with "10.10" OR MAC contains "BB:BB")
        // AND heartbeat count is exactly 1
        entry.device_id.contains("final")
            && (entry.ip.starts_with("10.10") || mac_address.to_string().contains("BB:BB"))
            && entry.heartbeat_count == 1
    });

    info!("Complex condition removed {} devices", removed);
    show_cache_state("After complex condition removal");

    // Final cleanup demonstration
    info!("\n10. Final cleanup - removing all remaining entries...");
    let removed = DeviceCacheManager::remove_entries_matching_mac(|_, _| true); // Remove all
    info!("Final cleanup removed {} devices", removed);
    show_cache_state("After final cleanup");
}
