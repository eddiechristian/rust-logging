# Device Cache Iteration Guide

This guide demonstrates how to work with the DEVICE_CACHE (a `LockFreeHashMap`) using both threading and async approaches.

## Overview

The `DEVICE_CACHE` is implemented using `lockfreehashmap::LockFreeHashMap` which provides thread-safe, lock-free operations. However, it has some limitations:

- **No direct iteration support**: The crate doesn't provide easy ways to iterate over all entries
- **Requires Guard objects**: All operations need a `Guard` obtained via `pin()`
- **Individual operations only**: Best suited for key-based get/set/remove operations

## Implementation

### DeviceCacheManager

We've implemented a `DeviceCacheManager` with the following capabilities:

```rust
use lockfreehashmap::pin;

impl DeviceCacheManager {
    /// Add a new device entry to cache
    pub fn add_device_entry(device_id: String, mac: String, ip: String, last_ping: Option<i32>) -> Result<()>
    
    /// Get a specific device entry from cache
    pub fn get_device_entry(device_id: &str) -> Option<DeviceCacheEntry>
    
    /// Update a device cache entry
    pub fn update_cache_entry(device_id: String, entry: DeviceCacheEntry) -> Result<()>
    
    /// Remove a specific device entry from cache
    pub fn remove_device_entry(device_id: &str) -> Option<DeviceCacheEntry>
    
    /// Start cache maintenance in a separate thread
    pub fn start_cache_maintenance_thread(cleanup_interval_seconds: u64, max_age_seconds: i64) -> thread::JoinHandle<()>
    
    /// Start async cache maintenance task
    pub async fn start_cache_maintenance_async(cleanup_interval_seconds: u64, max_age_seconds: i64)
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

## Alternative Approach for Full Iteration

Since `lockfreehashmap` doesn't support easy iteration, here's an alternative using `Arc<Mutex<HashMap>>`:

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

### Why lockfreehashmap?

**Pros:**
- True lock-free operations
- High performance for concurrent access
- No blocking between threads
- Memory efficient

**Cons:**
- No iteration support
- Complex API requiring Guards
- Limited operations

### When to Use Each Approach

1. **lockfreehashmap** - When you need:
   - High-performance concurrent access
   - Individual key-based operations
   - No need for full iteration

2. **Arc<Mutex<HashMap>>** - When you need:
   - Full iteration capabilities
   - Batch operations
   - Simpler API
   - Lower performance requirements

## Running the Examples

```bash
# Simple demonstration
cargo run --example simple_cache_demo

# Full-featured example with alternative approaches
cargo run --example cache_iteration
```

## Best Practices

1. **Always use Guards**: Every lockfreehashmap operation requires a guard from `pin()`
2. **Keep known device lists**: Since iteration is limited, maintain lists of device IDs elsewhere
3. **Design for individual operations**: Structure your code around get/set/remove patterns
4. **Consider alternatives**: For full iteration needs, evaluate `Arc<Mutex<HashMap>>` or other concurrent data structures
5. **Monitor performance**: The lock-free nature provides excellent performance for concurrent access

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

This approach provides both thread-based and async-based cache management while working within the constraints of the lockfreehashmap library.

