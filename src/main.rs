use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use offworld_trading_manager::config::load_config;
use offworld_trading_manager::consumer::spawn_send_consumer;
use offworld_trading_manager::models::PlanetStatus;
use offworld_trading_manager::pulsar::PulsarManager;
use offworld_trading_manager::routes::{
    connections_router, planets_router, settlements_router, space_elevator_router, stations_router,
    systems_router,
};
use offworld_trading_manager::state::{self, AppState};

#[derive(Parser, Debug)]
#[command(name = "offworld-trading-manager")]
#[command(about = "A space trading management server", long_about = None)]
struct Args {
    /// Path to a JSON file containing seed data
    #[arg(long)]
    seed: Option<String>,

    /// Port to listen on (can also be set via PORT env var)
    #[arg(short, long)]
    port: Option<u16>,

    /// Verbosity level (-v = warn, -vv = info, -vvv = debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Path to a TOML configuration file
    #[arg(long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = load_config(
        args.config.as_deref(),
        args.port,
        args.verbose,
        args.seed.as_deref(),
    );

    let log_level = match config.verbose {
        0 => "error",
        1 => "warn",
        2 => "info",
        _ => "debug",
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("offworld_trading_manager={}", log_level).into());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr = format!("0.0.0.0:{}", config.port);

    let galaxy = match &config.seed {
        Some(seed_path) => {
            info!(path = %seed_path, "Loading seed data from file");
            state::create_galaxy_state_from_file(seed_path).unwrap_or_else(|e| {
                error!(path = %seed_path, error = %e, "Failed to load seed file");
                std::process::exit(1);
            })
        }
        None => {
            info!("Using default seed data");
            state::create_galaxy_state()
        }
    };

    let config = Arc::new(config);

    // Try to connect to Pulsar
    let pulsar = match PulsarManager::new(config.pulsar.clone()).await {
        Ok(pm) => {
            info!("Pulsar connected successfully");
            Some(Arc::new(pm))
        }
        Err(e) => {
            warn!(error = %e, "Failed to connect to Pulsar, running without streaming");
            None
        }
    };

    let app_state = AppState {
        galaxy: galaxy.clone(),
        pulsar: pulsar.clone(),
        config: config.clone(),
    };

    // Spawn consumers for existing Connected stations if Pulsar is available
    if let Some(ref pulsar) = pulsar {
        let galaxy_read = galaxy.read().await;
        for (system_name, system) in &galaxy_read.systems {
            for planet in &system.planets {
                if matches!(planet.status, PlanetStatus::Connected { .. }) {
                    spawn_send_consumer(
                        galaxy.clone(),
                        pulsar.clone(),
                        config.clone(),
                        system_name.clone(),
                        planet.id.clone(),
                    );
                }
            }
        }
        drop(galaxy_read);
    }

    let app = Router::new()
        .nest("/systems", systems_router().merge(planets_router()))
        .nest(
            "/settlements",
            settlements_router()
                .merge(stations_router())
                .merge(space_elevator_router()),
        )
        .nest("/connections", connections_router())
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!(address = %addr, "Server running");
    axum::serve(listener, app).await.unwrap();
}
