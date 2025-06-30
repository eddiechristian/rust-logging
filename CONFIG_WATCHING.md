# Configuration File Watching

This application now supports automatic reinitialization when the `config.toml` file changes, while preserving the cache.

## How it works

1. **File Watcher**: The application uses the `notify` crate to monitor changes to `config.toml`
2. **Debounced Events**: Changes are debounced (500ms) to avoid rapid restarts during file edits
3. **Cache Preservation**: The `DeviceCacheManager` uses static/global storage, so cache data persists across restarts
4. **Graceful Restart**: Only the server instance restarts, not the entire process

## Features

- ✅ Monitors `config.toml` for changes
- ✅ Automatically restarts server when config changes
- ✅ Preserves cache data across restarts
- ✅ Debounced file watching (avoids rapid restarts)
- ✅ Graceful shutdown support (Ctrl+C, SIGTERM)
- ✅ Proper logging of restart events

## Usage

1. Start the server:
   ```bash
   cargo run
   ```

2. The server will display:
   ```
   File watcher started for config.toml
   Starting server with graceful shutdown and config reload support...
   ```

3. Edit `config.toml` (change port, database settings, etc.)

4. The server will automatically restart:
   ```
   Config file changed: "./config.toml"
   Config file changed, restarting server (cache will be preserved)...
   Server instance stopped
   Restarting server due to configuration change...
   Starting axum-health-service v0.1.0...
   ```

## Cache Behavior

- **Preserved**: All device cache entries remain intact during config reloads
- **Maintenance**: Cache maintenance tasks continue running
- **Statistics**: Performance statistics are maintained across restarts

## Testing

To test the functionality:

1. Start the server and make some requests to populate the cache
2. Check `/stats` endpoint to see cache data
3. Modify `config.toml` (e.g., change the port from 3000 to 3001)
4. Observe the automatic restart in the logs
5. Check `/stats` endpoint again - cache data should still be present
6. Server should now be running on the new port

## Dependencies Added

- `notify = "6.1"` - File system event monitoring
- `notify-debouncer-mini = "0.4"` - Debounced file watching

These dependencies enable efficient, non-blocking file system monitoring with proper debouncing to prevent rapid restarts during file editing.

