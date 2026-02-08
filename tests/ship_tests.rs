mod common;

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::{ALPHA_TOKEN, BETA_TOKEN};
use offworld_trading_manager::config::AppConfig;
use offworld_trading_manager::models::{
    CreateTruckingRequest, DockRequest, Ship, ShipStatus, UndockRequest,
};
use offworld_trading_manager::state::AppState;

fn player_auth(token: &str) -> String {
    format!("Bearer {}", token)
}

fn create_fast_ship_state() -> AppState {
    let seed_path = concat!(env!("CARGO_MANIFEST_DIR"), "/seed.json");
    let mut state = offworld_trading_manager::state::create_app_state_from_file(seed_path)
        .expect("Failed to load test seed data");

    let mut config = AppConfig::default();
    config.ship.au_to_seconds = 0.01;
    config.ship.seconds_per_unit = 0.001;
    config.ship.webhook_timeout_secs = 1;
    config.trucking.au_to_seconds = 0.01;
    config.trucking.jump_time_secs = 0.01;
    config.trucking.seconds_per_unit = 0.001;
    config.trucking.base_fee = 100;
    config.trucking.fee_per_unit = 1.0;
    config.admin.token = common::ADMIN_TOKEN.to_string();
    state.config = Arc::new(config);

    state
}

// Sol-3 (Earth) owned by alpha-team
// Proxima Centauri-1 (Proxima b) owned by beta-corp

#[tokio::test]
async fn test_trucking_full_lifecycle() {
    let state = create_fast_ship_state();

    // Create trucking ship from Earth (alpha) to Proxima b (beta)
    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 50);
            m
        },
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    let ship_id = ship.id;
    assert_eq!(ship.status, ShipStatus::InTransitToOrigin);
    assert!(ship.trucking_id.is_some());
    assert!(ship.fee.is_some());

    // Verify fee was deducted from player credits
    {
        let players = state.players.read().await;
        let alpha = players.get("alpha-team").unwrap();
        // Original 100000 - fee (100 + 50*1.0 = 150) = 99850
        assert_eq!(alpha.credits, 99850);
    }

    // Wait for transit to origin to complete (very fast config)
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Poll ship status - should be AwaitingOriginDockingAuth
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ships/{}", ship_id))
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::AwaitingOriginDockingAuth);

    // Verify cargo NOT yet deducted from origin station
    {
        let galaxy = state.galaxy.read().await;
        let sol = galaxy.systems.get("Sol").unwrap();
        let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
        if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
            earth.status
        {
            assert_eq!(station.inventory.get("iron_ore"), Some(&5000));
        }
    }

    // Dock at origin (alpha-team owns Sol-3)
    let dock = DockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/dock", ship_id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&dock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::Loading);

    // Verify cargo was NOW deducted from origin station
    {
        let galaxy = state.galaxy.read().await;
        let sol = galaxy.systems.get("Sol").unwrap();
        let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
        if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
            earth.status
        {
            assert_eq!(station.inventory.get("iron_ore"), Some(&4950));
        }
    }

    // Wait for loading to complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Poll to transition Loading → AwaitingOriginUndockingAuth
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ships/{}", ship_id))
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::AwaitingOriginUndockingAuth);

    // Undock from origin (alpha-team owns Sol-3)
    let undock = UndockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/undock", ship_id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&undock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::InTransit);

    // Wait for transit to destination to complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Poll ship status - should be AwaitingDockingAuth
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ships/{}", ship_id))
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::AwaitingDockingAuth);

    // Dock at destination (beta-corp owns Proxima Centauri-1)
    let dock = DockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/dock", ship_id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&dock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::Unloading);

    // Wait for unloading to complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Poll to transition Unloading → AwaitingUndockingAuth
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ships/{}", ship_id))
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::AwaitingUndockingAuth);

    // Undock from destination (beta-corp owns Proxima Centauri-1)
    let undock = UndockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/undock", ship_id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&undock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    assert_eq!(ship.status, ShipStatus::Complete);

    // Verify cargo was transferred to destination station
    let galaxy = state.galaxy.read().await;
    let proxima = galaxy.systems.get("Proxima Centauri").unwrap();
    let proxima_b = proxima
        .planets
        .iter()
        .find(|p| p.id == "Proxima Centauri-1")
        .unwrap();
    if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
        proxima_b.status
    {
        // Proxima b originally had 2000 iron_ore, should now have 2050
        assert_eq!(station.inventory.get("iron_ore"), Some(&2050));
    } else {
        panic!("Proxima b should be connected");
    }
}

#[tokio::test]
async fn test_trucking_same_station_rejected() {
    let state = create_fast_ship_state();
    let app = common::build_router(state);

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Sol-3".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 10);
            m
        },
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_trucking_fee_deduction() {
    let state = create_fast_ship_state();
    let app = common::build_router(state.clone());

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 100);
            m
        },
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    // Fee = base_fee(100) + 100 * fee_per_unit(1.0) = 200
    assert_eq!(ship.fee, Some(200));

    let players = state.players.read().await;
    let alpha = players.get("alpha-team").unwrap();
    assert_eq!(alpha.credits, 99800); // 100000 - 200
}

#[tokio::test]
async fn test_trucking_insufficient_credits() {
    let state = create_fast_ship_state();

    // Set alpha-team credits to 10 (not enough for base_fee of 100)
    {
        let mut players = state.players.write().await;
        players.get_mut("alpha-team").unwrap().credits = 10;
    }

    let app = common::build_router(state.clone());

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 10);
            m
        },
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_trucking_cargo_deducted_at_loading() {
    let state = create_fast_ship_state();

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 100);
            m
        },
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();
    let ship_id = ship.id;

    // Cargo should NOT be deducted yet (ship still in transit to origin)
    {
        let galaxy = state.galaxy.read().await;
        let sol = galaxy.systems.get("Sol").unwrap();
        let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
        if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
            earth.status
        {
            assert_eq!(station.inventory.get("iron_ore"), Some(&5000));
        }
    }

    // Wait for transit to origin
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Dock at origin - cargo deducted NOW
    let dock = DockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/dock", ship_id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&dock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify cargo was deducted
    {
        let galaxy = state.galaxy.read().await;
        let sol = galaxy.systems.get("Sol").unwrap();
        let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
        if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
            earth.status
        {
            assert_eq!(station.inventory.get("iron_ore"), Some(&4900));
        }
    }
}

#[tokio::test]
async fn test_dock_invalid_state() {
    let state = create_fast_ship_state();

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 10);
            m
        },
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/trucking")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ship: Ship = serde_json::from_slice(&body).unwrap();

    // Try to dock while still in transit to origin (should fail with 409)
    let dock = DockRequest { authorized: true };
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/ships/{}/dock", ship.id))
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&dock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_list_ships_with_filter() {
    let state = create_fast_ship_state();

    let create = CreateTruckingRequest {
        origin_planet_id: "Sol-3".to_string(),
        destination_planet_id: "Proxima Centauri-1".to_string(),
        cargo: {
            let mut m = HashMap::new();
            m.insert("iron_ore".to_string(), 10);
            m
        },
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/trucking")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::from(serde_json::to_string(&create).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    // List with player filter
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ships?player_id=alpha-team")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ships: Vec<Ship> = serde_json::from_slice(&body).unwrap();
    assert_eq!(ships.len(), 1);

    // player_id param is ignored; auth enforces ownership filtering
    // Alpha still sees their own ships regardless of player_id param
    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ships?player_id=nobody")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ships: Vec<Ship> = serde_json::from_slice(&body).unwrap();
    assert_eq!(ships.len(), 1);
}
