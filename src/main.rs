mod admin;
mod auth;
mod blockchain;
mod config;
mod db;
mod errors;
mod inspections;
mod middleware;
mod models;
mod notifications;
mod orders;
mod products;
mod routes;
mod seed;
mod seller;
mod services;
mod subscriptions;
mod trust;
mod utils;
mod wishlist;

use axum::http::{header, HeaderValue, Method};
use config::AppConfig;
use db::init_db_pool;
use models::AppState;
use routes::create_router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize structured logging framework
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,troit_logistics_backend=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting TROIT Logistics Backend Server...");

    // 2. Load environment configuration
    let config = AppConfig::from_env()?;
    info!(
        "Configuration loaded. Listening target: {}:{}",
        config.app_host, config.app_port
    );

    // 3. Connect to PostgreSQL and run automatic SQLx migrations
    let db = init_db_pool(&config.database_url).await?;

    // 4. Initialize Blockchain Service
    let blockchain_service = blockchain::BlockchainService::new(&config)?;
    let blockchain = std::sync::Arc::new(blockchain_service);

    // 5. Create AppState
    let state = AppState {
        db,
        config: config.clone(),
        blockchain,
    };

    // 5. Configure CORS middleware — restricted to the production frontend origin
    let frontend_origin: HeaderValue = "https://troit-logistics.vercel.app"
        .parse()
        .expect("invalid frontend origin");

    let cors = CorsLayer::new()
        .allow_origin(frontend_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    // 6. Configure per-IP rate limiting — protects auth, order and blockchain
    // endpoints from brute-force and abuse. Returns HTTP 429 when exceeded.
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(20)
            .finish()
            .expect("failed to build rate limiter configuration"),
    );

    let governor_limiter = governor_conf.limiter().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        governor_limiter.retain_recent();
    });

    // 7. Build Axum Router with middleware layers
    let app = create_router(state)
        .layer(cors)
        .layer(GovernorLayer {
            config: governor_conf,
        })
        .layer(TraceLayer::new_for_http());

    // 8. Bind TCP listener and serve
    let bind_address = format!("{}:{}", config.app_host, config.app_port);
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;

    info!(
        "🚀 TROIT Logistics Backend running successfully on http://{}",
        bind_address
    );
    info!(
        "Health check endpoint available at: http://{}/health",
        bind_address
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
