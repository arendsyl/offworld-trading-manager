use axum::Router;

use offworld_trading_manager::routes::{
    connections_router, planets_router, settlements_router, space_elevator_router, stations_router,
    systems_router,
};
use offworld_trading_manager::state::{self, AppState};

const TEST_SEED_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/seed.json");

pub fn create_test_app() -> Router {
    let state = state::create_app_state_from_file(TEST_SEED_FILE).expect("Failed to load test seed data");
    Router::new()
        .nest("/systems", systems_router().merge(planets_router()))
        .nest(
            "/settlements",
            settlements_router()
                .merge(stations_router())
                .merge(space_elevator_router()),
        )
        .nest("/connections", connections_router())
        .with_state(state)
}

pub fn create_test_app_with_state() -> (Router, AppState) {
    let state = state::create_app_state_from_file(TEST_SEED_FILE).expect("Failed to load test seed data");
    let app = Router::new()
        .nest("/systems", systems_router().merge(planets_router()))
        .nest(
            "/settlements",
            settlements_router()
                .merge(stations_router())
                .merge(space_elevator_router()),
        )
        .nest("/connections", connections_router())
        .with_state(state.clone());
    (app, state)
}
