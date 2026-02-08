use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use offworld_trading_manager::config::load_config;
use offworld_trading_manager::consumer::spawn_send_consumer;
use offworld_trading_manager::market::MarketState;
use offworld_trading_manager::pulsar::PulsarManager;
use offworld_trading_manager::auth::{admin_auth_middleware, player_auth_middleware};
use offworld_trading_manager::routes::{
    admin_connections_router, admin_planets_router, admin_players_router,
    admin_settlements_router, admin_stations_router, admin_systems_router,
    player_construction_router, player_leaderboard_router, player_market_router, player_planets_router, player_trade_router,
    player_players_router, player_settlements_router, player_ships_router,
    player_stations_router, player_systems_router, player_trucking_router,
    space_elevator_router,
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

    // Load players from seed data
    let mut players = match &config.seed {
        Some(seed_path) => {
            state::load_players_from_seed(seed_path).unwrap_or_else(|e| {
                warn!(error = %e, "Failed to load players from seed data");
                HashMap::new()
            })
        }
        None => HashMap::new(),
    };

    // Parse Biscuit root key from config
    let biscuit_root = {
        use biscuit_auth::{PrivateKey, KeyPair};
        let private_key = PrivateKey::from_bytes_hex(
            &config.admin.biscuit_private_key_hex,
        )
        .unwrap_or_else(|e| {
            error!(error = %e, "Invalid biscuit private key hex");
            std::process::exit(1);
        });
        Arc::new(KeyPair::from(&private_key))
    };

    // Generate Biscuit tokens for seed players that don't have one
    {
        use biscuit_auth::macros::biscuit;
        for player in players.values_mut() {
            if player.pulsar_biscuit.is_empty() {
                let topic_receive = format!(
                    "persistent://{}/{}/mass-driver.receive.{}",
                    config.pulsar.tenant, config.pulsar.namespace, player.id
                );
                let topic_send = format!(
                    "persistent://{}/{}/mass-driver.send.{}",
                    config.pulsar.tenant, config.pulsar.namespace, player.id
                );
                let player_id = player.id.as_str();
                let token = biscuit!(
                    r#"
                    player({player_id});
                    topic({topic_receive});
                    topic({topic_send});
                    "#
                )
                .build(&biscuit_root)
                .expect("failed to build biscuit token");
                player.pulsar_biscuit = token.to_base64().expect("failed to serialize biscuit");
            }
        }
    }

    let trade_channel_capacity = config.market.trade_channel_capacity;
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
        players: Arc::new(RwLock::new(players)),
        ships: Arc::new(RwLock::new(HashMap::new())),
        projects: Arc::new(RwLock::new(HashMap::new())),
        trade_requests: Arc::new(RwLock::new(HashMap::new())),
        market: Arc::new(RwLock::new(MarketState::new(trade_channel_capacity))),
        pulsar: pulsar.clone(),
        config: config.clone(),
        http_client: reqwest::Client::new(),
        biscuit_root,
    };

    // Spawn consumers for each player if Pulsar is available
    if let Some(ref pulsar) = pulsar {
        let players_read = app_state.players.read().await;
        for player_id in players_read.keys() {
            spawn_send_consumer(
                galaxy.clone(),
                pulsar.clone(),
                config.clone(),
                player_id.clone(),
            );
        }
        drop(players_read);
    }

    let admin_router = Router::new()
        .nest("/systems", admin_systems_router().merge(admin_planets_router()))
        .nest(
            "/settlements",
            admin_settlements_router().merge(admin_stations_router()),
        )
        .nest("/connections", admin_connections_router())
        .nest("/players", admin_players_router())
        .layer(axum::middleware::from_fn_with_state(app_state.clone(), admin_auth_middleware));

    let player_router = Router::new()
        .nest("/systems", player_systems_router().merge(player_planets_router()))
        .nest(
            "/settlements",
            player_settlements_router()
                .merge(player_stations_router())
                .merge(space_elevator_router()),
        )
        .nest("/players", player_players_router())
        .nest("/ships", player_ships_router())
        .nest("/trucking", player_trucking_router())
        .nest("/market", player_market_router())
        .nest("/construction", player_construction_router())
        .nest("/trade", player_trade_router())
        .nest("/leaderboard", player_leaderboard_router())
        .layer(axum::middleware::from_fn_with_state(app_state.clone(), player_auth_middleware));

    let app = Router::new()
        .nest("/admin", admin_router)
        .merge(player_router)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!(address = %addr, "Server running");
    axum::serve(listener, app).await.unwrap();
}
