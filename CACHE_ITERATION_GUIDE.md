# Device Cache Iteration and Deletion Guide

This guide demonstrates how to work with the DEVICE_CACHE (a `DashMap`) using both threading and async approaches, with comprehensive support for iteration and conditional deletion.

## Overview

The `DEVICE_CACHE` is implemented using `dashmap::DashMap` which provides excellent thread-safe, concurrent operations with full iteration support:

- **Full iteration support**: Easy iteration over all entries with `.iter()` and `.iter_mut()`
- **Simple API**: No complex guard objects or special APIs required
- **High performance**: Lock-free operations with excellent concurrent performance
- **Conditional deletion**: Rich support for removing entries based on various criteria

## Implementation

### DeviceCacheManager

We've implemented a `DeviceCacheManager` with the following capabilities:

```rust
impl DeviceCacheManager {
    // Basic Operations
    pub fn add_device_entry(device_id: String, mac: String, ip: String, last_ping: Option<i32>) -> Result<()>
    pub fn get_device_entry(device_id: &str) -> Option<DeviceCacheEntry>
    pub fn update_cache_entry(device_id: String, entry: DeviceCacheEntry) -> Result<()>
    pub fn remove_device_entry(device_id: &str) -> Option<DeviceCacheEntry>
    
    // Iteration and Batch Operations
    pub fn get_cache_snapshot() -> Vec<(String, DeviceCacheEntry)>
    pub fn iterate_cache_entries<F>(callback: F) where F: FnMut(&String, &DeviceCacheEntry)
    pub fn update_all_entries<F>(updater: F) -> usize where F: FnMut(&String, &mut DeviceCacheEntry) -> bool
    
    // Conditional Deletion
    pub fn remove_entries_matching<F>(predicate: F) -> usize where F: Fn(&String, &DeviceCacheEntry) -> bool
    pub fn remove_entries_by_ip_pattern(ip_pattern: &str) -> usize
    pub fn remove_entries_by_mac_pattern(mac_pattern: &str) -> usize
    pub fn remove_entries_by_device_pattern(device_pattern: &str) -> usize
    pub fn remove_entries_with_low_heartbeats(min_heartbeats: u64) -> usize
    pub fn remove_entries_older_than(max_age_seconds: i64) -> usize
    pub fn iterate_and_remove_with_logging<F>(condition: F) -> (usize, usize)
    pub fn remove_entries_advanced_criteria(...) -> usize
    
    // Maintenance Tasks
    pub fn start_cache_maintenance_thread(cleanup_interval_seconds: u64, max_age_seconds: i64) -> thread::JoinHandle<()>
    pub async fn start_cache_maintenance_async(cleanup_interval_seconds: u64, max_age_seconds: i64)
    
    // Statistics
    pub fn get_cache_stats() -> CacheStats
    pub fn get_cache_size() -> usize
}
```

## Usage Examples

### 1. Threading Approach

```rust
use std::thread;
use std::time::Duration;

// Start a background thread for cache operations
let cache_thread = thread::spawn(|| {
    loop {
        // Add devices
        for i in 1..=10 {
            let device_id = format!("device_{:03}", i);
            let mac = format!("00:11:22:33:44:{:02}", i);
            let ip = format!("192.168.1.{}", 100 + i);
            
            DeviceCacheManager::add_device_entry(device_id, mac, ip, Some(80));
        }
        
        // Process existing devices
        for i in 1..=10 {
            let device_id = format!("device_{:03}", i);
            
            if let Some(entry) = DeviceCacheManager::get_device_entry(&device_id) {
                // Update the entry
                DeviceCacheManager::update_cache_entry(device_id, entry);
            }
        }
        
        thread::sleep(Duration::from_secs(30));
    }
});
```

### 2. Async Approach

```rust
use tokio::time::{interval, Duration};

// Start an async task for cache operations
tokio::spawn(async {
    let mut interval_timer = interval(Duration::from_secs(30));
    
    loop {
        interval_timer.tick().await;
        
        // Add devices
        for i in 1..=10 {
            let device_id = format!("async_device_{:03}", i);
            let mac = format!("AA:BB:CC:DD:EE:{:02}", i);
            let ip = format!("10.0.0.{}", 10 + i);
            
            DeviceCacheManager::add_device_entry(device_id, mac, ip, Some(443)).await;
        }
        
        // Process existing devices
        for i in 1..=10 {
            let device_id = format!("async_device_{:03}", i);
            
            if let Some(entry) = DeviceCacheManager::get_device_entry(&device_id) {
                // Update the entry
                DeviceCacheManager::update_cache_entry(device_id, entry).await;
            }
        }
    }
});
```

### 3. Integration in Main Application

```rust
// In your main function, start cache maintenance

// Option 1: Thread-based maintenance
let _cache_thread = DeviceCacheManager::start_cache_maintenance_thread(
    300,  // Clean every 5 minutes
    1800, // Remove entries older than 30 minutes
);

// Option 2: Async maintenance
tokio::spawn(async {
    DeviceCacheManager::start_cache_maintenance_async(
        300,  // Clean every 5 minutes
        1800, // Remove entries older than 30 minutes
    ).await;
});
```

## Conditional Deletion Examples

DashMap excels at conditional deletion operations. Here are various patterns:

### Basic Pattern Matching

```rust
// Remove by IP pattern
let removed = DeviceCacheManager::remove_entries_by_ip_pattern("10.0.0.");

// Remove by MAC pattern  
let removed = DeviceCacheManager::remove_entries_by_mac_pattern("AA:BB:CC");

// Remove by device name pattern
let removed = DeviceCacheManager::remove_entries_by_device_pattern("test_");

// Remove by heartbeat count
let removed = DeviceCacheManager::remove_entries_with_low_heartbeats(5);

// Remove by age
let removed = DeviceCacheManager::remove_entries_older_than(3600); // 1 hour
```

### Custom Conditions

```rust
// Remove with custom condition
let removed = DeviceCacheManager::remove_entries_matching(|device_id, entry| {
    // Complex condition: remove if device is a test device with low heartbeats
    device_id.contains("test") && entry.heartbeat_count < 3
});

// Remove with detailed logging
let (checked, removed) = DeviceCacheManager::iterate_and_remove_with_logging(|device_id, entry| {
    let current_time = chrono::Utc::now().timestamp();
    let age = current_time - entry.last_seen;
    
    // Remove if device hasn't been seen for 30 minutes and has low heartbeats
    age > 1800 && entry.heartbeat_count < 10
});
```

### Advanced Multi-Criteria Deletion

```rust
// Remove using multiple criteria
let removed = DeviceCacheManager::remove_entries_advanced_criteria(
    Some(3600),                           // Older than 1 hour
    Some(5),                             // Less than 5 heartbeats  
    Some(&["10.0.0", "192.168.99"]),     // IP patterns
    Some(&["AA:BB:CC", "FF:EE:DD"]),     // MAC patterns
    Some(&["test", "temp", "dev"]),      // Device name patterns
);
```

## Alternative Approach Comparison

While DashMap provides excellent iteration support, here's a comparison with traditional approaches:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
            cache.insert(id, entry);
        }
    }
    
    // Full iteration support
    fn iterate_all_devices(&self) -> Vec<(String, DeviceCacheEntry)> {
        if let Ok(cache) = self.cache.lock() {
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            Vec::new()
        }
    }
    
    // Update all devices in a batch
    fn update_all_devices<F>(&self, updater: F) 
    where 
        F: Fn(&mut DeviceCacheEntry),
    {
        if let Ok(mut cache) = self.cache.lock() {
            for entry in cache.values_mut() {
                updater(entry);
            }
        }
    }
}
```

## Key Design Decisions

### Why DashMap?

**Pros:**
- Excellent concurrent performance
- Full iteration and conditional deletion support
- Simple, intuitive API
- Lock-free reads, minimal locking for writes
- Rich ecosystem of operations

**Cons:**
- Slightly more memory overhead than some alternatives
- Write operations may have brief locking (but optimized)

### When to Use Each Approach

1. **DashMap** (Recommended) - When you need:
   - High-performance concurrent access
   - Full iteration capabilities
   - Conditional deletion operations
   - Simple API without complex guards
   - Production-ready concurrent caching

2. **Arc<Mutex<HashMap>>** - When you need:
   - Maximum simplicity
   - Atomic batch operations across entire map
   - Lower memory usage
   - Non-performance-critical scenarios

## Running the Examples

```bash
# Simple demonstration with full iteration
cargo run --example simple_cache_demo

# Full-featured example with threading and async
cargo run --example cache_iteration

# Comprehensive conditional deletion demonstration
cargo run --example cache_deletion_demo
```

## Best Practices

1. **Use appropriate deletion methods**: Choose the right deletion method for your use case (pattern matching vs custom conditions)
2. **Batch operations efficiently**: Use `update_all_entries()` for bulk updates rather than individual operations
3. **Monitor cache size**: Use `get_cache_stats()` to monitor cache health and performance
4. **Implement proper cleanup**: Use scheduled cleanup tasks to prevent memory leaks
5. **Log important operations**: Use `iterate_and_remove_with_logging()` for critical deletion operations
6. **Test deletion conditions**: Always test your deletion predicates thoroughly
7. **Consider performance**: DashMap provides excellent concurrent performance for real-world scenarios

## Integration with Your Application

The cache management can be integrated into your existing axum application:

```rust
// In main.rs
// Start background cache maintenance
let _cache_maintenance = DeviceCacheManager::start_cache_maintenance_thread(300, 1800);

// In your HBD endpoint handler
if let Some(mut entry) = DeviceCacheManager::get_device_entry(&device_id) {
    // Update the entry with new heartbeat data
    DeviceCacheManager::update_cache_entry(device_id, entry)?;
} else {
    // Add new device to cache
    DeviceCacheManager::add_device_entry(device_id, mac, ip, last_ping)?;
}
```

This approach provides both thread-based and async-based cache management with full iteration and conditional deletion capabilities using DashMap's excellent concurrent performance.

