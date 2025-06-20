use log::{error, info};
use mysql::{Pool, OptsBuilder};
use mysql::prelude::Queryable;
use std::net::SocketAddr;
use tokio;

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

#[tokio::main]
async fn main() {
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
        Ok(mut conn) => {
            match conn.query_drop("SELECT 1") {
                Ok(_) => info!("Database connectivity test successful"),
                Err(e) => {
                    error!("Database connectivity test failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            error!("Failed to get initial database connection: {}", e);
            std::process::exit(1);
        }
    }

    // Build our application with routes
    let app = server::create_router(db_pool);

    // Server address from config
    let bind_addr = config.bind_address();
    let addr: SocketAddr = bind_addr.parse().unwrap_or_else(|_| {
        error!("Invalid bind address in config: {}", bind_addr);
        std::process::exit(1);
    });
    
    info!("Server will listen on: {}", addr);
    info!("Health endpoint available at: http://{}/health", addr);
    info!("HBD endpoint available at: http://{}/hbd", addr);
    info!("Server protocol: HTTP/1.1");
    info!("Server framework: Axum v0.7");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        error!("Failed to bind to {}: {}", addr, e);
        std::process::exit(1);
    });
    
    info!("TCP listener bound successfully to {}", addr);
    info!("Server is ready to accept connections...");

    // Serve with connection info
    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        error!("Server error: {}", e);
    }
}
