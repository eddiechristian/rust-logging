use log::{error, info, warn};
use mysql::prelude::Queryable;
use mysql::{OptsBuilder, Pool};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio;
use tokio::sync::mpsc;

mod app;
mod config;
mod server;

fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    use log4rs::{
        append::console::ConsoleAppender,
        config::{Appender, Config, Root},
        encode::pattern::PatternEncoder,
    };

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} [{h({l})}] {t} - {m}{n}",
        )))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(
            Root::builder()
                .appender("stdout")
                .build(log::LevelFilter::Info),
        )?;

    log4rs::init_config(config)?;
    Ok(())
}

async fn run_server() -> bool {
    // Load configuration
    let config = match config::Config::load_or_default("config.toml") {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    if let Err(e) = init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    info!("Starting {} v{}...", config.app.name, config.app.version);
    info!("Configuration loaded from config.toml");
    info!("Database URL: {}", config.database_url());

    // Create database connection pool
    let db_opts = OptsBuilder::new()
        .ip_or_hostname(Some(&config.database.host))
        .tcp_port(config.database.port)
        .user(Some(&config.database.username))
        .pass(Some(&config.database.password))
        .db_name(Some(&config.database.database));

    let db_pool = match Pool::new(db_opts) {
        Ok(pool) => {
            info!("Database connection pool created successfully");
            info!("Pool size: {}", config.database.pool_size);
            pool
        }
        Err(e) => {
            error!("Failed to create database connection pool: {}", e);
            std::process::exit(1);
        }
    };

    // Test database connectivity
    match db_pool.get_conn() {
        Ok(mut conn) => match conn.query_drop("SELECT 1") {
            Ok(_) => info!("Database connectivity test successful"),
            Err(e) => {
                error!("Database connectivity test failed: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            error!("Failed to get initial database connection: {}", e);
            std::process::exit(1);
        }
    }

    // Build our application with routes
    let app = server::create_router(db_pool);

    // Start cache maintenance tasks (cache is preserved across restarts)
    info!("Starting device cache maintenance tasks");
    
    // Option 1: Start cache maintenance in a separate thread
    let _cache_thread = app::DeviceCacheManager::start_cache_maintenance_thread(
        300,  // Clean every 5 minutes
        1800, // Remove entries older than 30 minutes
    );
    
    // Option 2: Start async cache maintenance task
    tokio::spawn(async {
        app::DeviceCacheManager::start_cache_maintenance_async(
            300,  // Clean every 5 minutes
            1800, // Remove entries older than 30 minutes
        ).await;
    });
    
    info!("Cache maintenance tasks started");

    // Server address from config
    let bind_addr = config.bind_address();
    let addr: SocketAddr = bind_addr.parse().unwrap_or_else(|_| {
        error!("Invalid bind address in config: {}", bind_addr);
        std::process::exit(1);
    });

    info!("Server will listen on: {}", addr);
    info!("Health endpoint available at: http://{}/health", addr);
    info!("HBD endpoint available at: http://{}/hbd", addr);
    info!("Stats endpoint available at: http://{}/stats", addr);
    info!("Stats reset endpoint available at: http://{}/stats/reset", addr);
    info!("Server protocol: HTTP/1.1");
    info!("Server framework: Axum v0.7");
    info!("Performance monitoring: Enabled (AtomicCell-based)");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        });

    info!("TCP listener bound successfully to {}", addr);
    info!("Server is ready to accept connections...");
    info!("Press Ctrl+C to shutdown gracefully");

    // Create channels for shutdown and config reload signals
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (config_reload_tx, mut config_reload_rx) = mpsc::channel::<()>(1);

    // Set up file watcher for config.toml
    let config_path = Path::new("config.toml");
    let config_reload_tx_clone = config_reload_tx.clone();
    
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel(1);
        
        let tx_clone = tx.clone();
        let mut debouncer = match new_debouncer(
            Duration::from_millis(500),
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    for event in events {
                        if event.path.file_name().and_then(|n| n.to_str()) == Some("config.toml") {
                            info!("Config file changed: {:?}", event.path);
                            if let Err(e) = tx_clone.blocking_send(()) {
                                error!("Failed to send config reload signal: {}", e);
                            }
                        }
                    }
                }
            }
        ) {
            Ok(debouncer) => debouncer,
            Err(e) => {
                error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer.watcher().watch(config_path.parent().unwrap_or(Path::new(".")), RecursiveMode::NonRecursive) {
            error!("Failed to watch config directory: {}", e);
            return;
        }

        info!("File watcher started for config.toml");
        
        while let Some(_) = rx.recv().await {
            if let Err(e) = config_reload_tx_clone.send(()).await {
                error!("Failed to send config reload signal: {}", e);
                break;
            }
        }
    });

    // Create graceful shutdown signal
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
            _ = terminate => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
        }
        
        let _ = shutdown_tx_clone.send(()).await;
    });

    // Main server loop with config reload support
    info!("Starting server with graceful shutdown and config reload support...");
    
    let should_restart = loop {
        tokio::select! {
            // Handle server shutdown signal
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping server...");
                break false; // Don't restart
            }
            
            // Handle config reload signal
            _ = config_reload_rx.recv() => {
                warn!("Config file changed, restarting server (cache will be preserved)...");
                // Note: The cache (DeviceCacheManager) is static/global and will persist
                // across this restart since we're not exiting the process
                break true; // Restart
            }
            
            // Run the server
            result = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            ) => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
                break false; // Don't restart on server error
            }
        }
    };

    info!("Server instance stopped");
    should_restart
}

#[tokio::main]
async fn main() {
    loop {
        let should_restart = run_server().await;
        
        if should_restart {
            info!("Restarting server due to configuration change...");
            tokio::time::sleep(Duration::from_millis(1000)).await; // Brief pause before restart
        } else {
            info!("Server shutdown complete. Goodbye!");
            break;
        }
    }
}
