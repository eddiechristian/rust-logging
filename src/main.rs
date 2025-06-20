use log::{error, info};
use std::net::SocketAddr;
use tokio;

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
    // Initialize logging
    if let Err(e) = init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    info!("Starting Axum health service...");

    // Build our application with routes
    let app = server::create_router();

    // Server address
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server will listen on: {}", addr);
    info!("Health endpoint available at: http://{}/health", addr);
    info!("Server protocol: HTTP/1.1");
    info!("Server framework: Axum v0.7");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
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
