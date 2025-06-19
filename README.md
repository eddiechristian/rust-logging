# Axum Health Service

A simple Rust web service built with Axum that provides a health check endpoint with log4rs logging.

## Features

- **Axum Web Framework**: Fast and ergonomic web framework for Rust
- **Health Endpoint**: `/health` endpoint that returns service status
- **log4rs Logging**: Structured logging with configurable output
- **JSON Responses**: Health endpoint returns JSON with status and timestamp

## Dependencies

- `axum` - Web framework
- `tokio` - Async runtime
- `log4rs` - Logging framework
- `log` - Logging facade
- `serde` - Serialization framework
- `chrono` - Date and time handling

## Running the Service

1. Build the project:
   ```bash
   cargo build
   ```

2. Run the service:
   ```bash
   cargo run
   ```

3. The service will start on `http://127.0.0.1:3000`

## API Endpoints

### Health Check

**GET** `/health`

Returns the health status of the service.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-01T12:00:00.000Z"
}
```

**Example:**
```bash
curl http://127.0.0.1:3000/health
```

## Logging

The service uses log4rs for logging with the following features:
- Console output with timestamp, log level, and message
- Info level logging by default
- Logs when the health endpoint is called
- Logs server startup and any errors

## Development

To modify the logging configuration, edit the `init_logging()` function in `src/main.rs`. You can:
- Change log levels
- Add file output
- Modify log formatting
- Add additional appenders

