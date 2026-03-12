use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use offworld_trading_manager::api_doc::ApiDoc;
use offworld_trading_manager::config::load_config;
use offworld_trading_manager::economy;
use offworld_trading_manager::consumer::spawn_send_consumer;
use offworld_trading_manager::market::MarketState;
use offworld_trading_manager::persistence;
use offworld_trading_manager::pulsar::PulsarManager;
use offworld_trading_manager::auth::{admin_auth_middleware, player_auth_middleware};
use offworld_trading_manager::routes::{
    admin_connections_router, admin_persistence_router, admin_planets_router, admin_players_router,
    admin_settlements_router, admin_stations_router, admin_systems_router,
    player_projects_router, player_leaderboard_router, player_market_router, player_planets_router, player_trade_router,
    player_economy_router, player_players_router, player_settlements_router,
    player_ships_router, player_stations_router, player_systems_router,
    player_trucking_router, space_elevator_router,
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

    /// Save name for S3 persistence (enables save/load from S3)
    #[arg(long)]
    save: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut config = load_config(
        args.config.as_deref(),
        args.port,
        args.verbose,
        args.seed.as_deref(),
    );

    // Set save_name from CLI arg
    if args.save.is_some() {
        config.save_name = args.save;
    }

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

    // Build S3 bucket handle if configured
    let s3_bucket = if config.s3.bucket.is_some() {
        match persistence::build_s3_bucket(&config.s3) {
            Ok(bucket) => {
                info!("S3 bucket configured");
                Some(bucket)
            }
            Err(e) => {
                warn!(error = %e, "Failed to configure S3 bucket, running without persistence");
                None
            }
        }
    } else {
        None
    };

    let save_name = config.save_name.clone();
    let auto_save_interval = config.s3.auto_save_interval_secs;
    let trade_channel_capacity = config.market.trade_channel_capacity;
    let config = Arc::new(config);

    // Determine if we should load from S3
    let should_load_from_s3 = if let (Some(bucket), Some(name)) = (&s3_bucket, &save_name)
    {
        persistence::check_save_exists(bucket, name).await
    } else {
        false
    };

    let app_state = if should_load_from_s3 {
        let bucket_ref = s3_bucket.as_ref().unwrap();
        let save_name_ref = save_name.as_deref().unwrap();

        info!(save_name = save_name_ref, "Loading game state from S3");

        // Create empty AppState with S3 bucket
        let app_state = AppState {
            galaxy: Arc::new(RwLock::new(state::GalaxyState::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
            ships: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            trade_requests: Arc::new(RwLock::new(HashMap::new())),
            market: Arc::new(RwLock::new(MarketState::new(trade_channel_capacity))),
            pulsar: None,
            config: config.clone(),
            http_client: reqwest::Client::new(),
            biscuit_root: Arc::new(biscuit_auth::KeyPair::new()),
            s3: s3_bucket.clone(),
        };

        // Load snapshot
        persistence::load_snapshot(&app_state, bucket_ref, save_name_ref)
            .await
            .unwrap_or_else(|e| {
                error!(error = %e, "Failed to load snapshot from S3");
                std::process::exit(1);
            });

        // Parse Biscuit root key and generate tokens for loaded players
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

        {
            use biscuit_auth::macros::biscuit;
            let mut players = app_state.players.write().await;
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
                    .unwrap_or_else(|e| {
                        error!(player_id = %player.id, error = %e, "Failed to build biscuit token");
                        std::process::exit(1);
                    });
                    player.pulsar_biscuit = token.to_base64().unwrap_or_else(|e| {
                        error!(player_id = %player.id, error = %e, "Failed to serialize biscuit");
                        std::process::exit(1);
                    });
                }
            }
        }

        // Replace biscuit_root
        let app_state = AppState {
            biscuit_root,
            ..app_state
        };

        // Spawn economy loop
        economy::spawn_economy_loop(app_state.galaxy.clone(), app_state.config.clone());

        // Recover in-flight tasks
        persistence::recover_in_flight_tasks(&app_state).await;

        app_state
    } else {
        // Standard startup: load from seed
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
                    .unwrap_or_else(|e| {
                        error!(player_id = %player.id, error = %e, "Failed to build biscuit token");
                        std::process::exit(1);
                    });
                    player.pulsar_biscuit = token.to_base64().unwrap_or_else(|e| {
                        error!(player_id = %player.id, error = %e, "Failed to serialize biscuit");
                        std::process::exit(1);
                    });
                }
            }
        }

        let app_state = AppState {
            galaxy: galaxy.clone(),
            players: Arc::new(RwLock::new(players)),
            ships: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            trade_requests: Arc::new(RwLock::new(HashMap::new())),
            market: Arc::new(RwLock::new(MarketState::new(trade_channel_capacity))),
            pulsar: None,
            config: config.clone(),
            http_client: reqwest::Client::new(),
            biscuit_root,
            s3: s3_bucket.clone(),
        };

        // Spawn economy simulation loop
        economy::spawn_economy_loop(app_state.galaxy.clone(), app_state.config.clone());

        app_state
    };

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
        pulsar: pulsar.clone(),
        ..app_state
    };

    // Spawn consumers for each player if Pulsar is available
    if let Some(ref pulsar) = pulsar {
        let galaxy = app_state.galaxy.clone();
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
        .nest("/persistence", admin_persistence_router())
        .layer(axum::middleware::from_fn_with_state(app_state.clone(), admin_auth_middleware));

    let player_router = Router::new()
        .nest("/systems", player_systems_router().merge(player_planets_router()))
        .nest(
            "/settlements",
            player_settlements_router()
                .merge(player_stations_router())
                .merge(space_elevator_router())
                .merge(player_economy_router()),
        )
        .nest("/players", player_players_router())
        .nest("/ships", player_ships_router())
        .nest("/trucking", player_trucking_router())
        .nest("/market", player_market_router())
        .nest("/projects", player_projects_router())
        .nest("/trade", player_trade_router())
        .nest("/leaderboard", player_leaderboard_router())
        .layer(axum::middleware::from_fn_with_state(app_state.clone(), player_auth_middleware));

    // Spawn auto-save task if configured
    if let (Some(bucket), Some(name), Some(interval_secs)) =
        (&s3_bucket, &save_name, auto_save_interval)
    {
        let state_for_save = app_state.clone();
        let bucket_for_save = bucket.clone();
        let name_for_save = name.clone();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(interval_secs);
            loop {
                tokio::time::sleep(interval).await;
                match persistence::save_snapshot(
                    &state_for_save,
                    &bucket_for_save,
                    &name_for_save,
                )
                .await
                {
                    Ok(()) => info!("Auto-save completed"),
                    Err(e) => error!(error = %e, "Auto-save failed"),
                }
            }
        });
        info!(interval_secs = interval_secs, "Auto-save task started");
    }

    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", ApiDoc::openapi()))
        .nest("/admin", admin_router)
        .merge(player_router)
        .with_state(app_state.clone());

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        error!(address = %addr, error = %e, "Failed to bind to address");
        std::process::exit(1);
    });
    info!(address = %addr, "Server running");

    // Graceful shutdown
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = ctrl_c => info!("SIGINT received, shutting down"),
                _ = sigterm.recv() => info!("SIGTERM received, shutting down"),
            }
        }
        #[cfg(not(unix))]
        {
            ctrl_c.await.ok();
            info!("SIGINT received, shutting down");
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap_or_else(|e| {
            error!(error = %e, "Server error");
            std::process::exit(1);
        });

    // Shutdown save
    if let (Some(bucket), Some(name)) = (&s3_bucket, &save_name) {
        match persistence::save_snapshot(&app_state, bucket, name).await {
            Ok(()) => info!("Shutdown save complete"),
            Err(e) => error!(error = %e, "Shutdown save failed"),
        }
    }
}
