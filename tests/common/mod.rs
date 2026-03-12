use std::sync::Arc;

use axum::Router;

use offworld_trading_manager::auth::{admin_auth_middleware, player_auth_middleware};
use offworld_trading_manager::config::AppConfig;
use offworld_trading_manager::routes::{
    admin_connections_router, admin_planets_router, admin_players_router,
    admin_settlements_router, admin_stations_router, admin_systems_router,
    player_economy_router, player_market_router, player_planets_router, player_players_router,
    player_settlements_router, player_ships_router, player_stations_router,
    player_systems_router, player_trade_router, player_trucking_router, space_elevator_router,
};
use offworld_trading_manager::state::{self, AppState};

const TEST_SEED_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/seed.json");

pub const ADMIN_TOKEN: &str = "test-admin-token";
pub const ALPHA_TOKEN: &str = "alpha-secret-key-001";
pub const BETA_TOKEN: &str = "beta-secret-key-002";

pub fn create_test_app() -> Router {
    let state = create_test_state();
    build_router(state)
}

pub fn create_test_app_with_state() -> (Router, AppState) {
    let state = create_test_state();
    let app = build_router(state.clone());
    (app, state)
}

fn create_test_state() -> AppState {
    let mut state = state::create_app_state_from_file(TEST_SEED_FILE)
        .expect("Failed to load test seed data");
    let mut config = AppConfig::default();
    config.admin.token = ADMIN_TOKEN.to_string();
    state.config = Arc::new(config);
    state
}

pub fn build_router(state: AppState) -> Router {
    let admin_router = Router::new()
        .nest("/systems", admin_systems_router().merge(admin_planets_router()))
        .nest(
            "/settlements",
            admin_settlements_router().merge(admin_stations_router()),
        )
        .nest("/connections", admin_connections_router())
        .nest("/players", admin_players_router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), admin_auth_middleware));

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
        .nest("/trade", player_trade_router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), player_auth_middleware));

    Router::new()
        .nest("/admin", admin_router)
        .merge(player_router)
        .with_state(state)
}
