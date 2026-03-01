use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use offworld_trading_manager::models::PlanetStatus;
use serde_json::{json, Value};
use std::time::Instant;
use tower::ServiceExt;

mod common;

use common::{ADMIN_TOKEN, ALPHA_TOKEN};

fn admin_auth() -> String {
    format!("Bearer {}", ADMIN_TOKEN)
}

fn player_auth(token: &str) -> String {
    format!("Bearer {}", token)
}

#[tokio::test]
async fn test_get_space_elevator_not_connected() {
    let app = common::create_test_app();

    // Venus (Sol-2) is uninhabited, so it returns 404 (not found)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settlements/Sol/Sol-2/space-elevator")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Uninhabited planets return 404 (not found in settlements)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_space_elevator_after_station_created() {
    let (app, _state) = common::create_test_app_with_state();

    // Create station on Mars via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get the space elevator as the station owner
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settlements/Sol/Sol-4/space-elevator")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let elevator: Value = serde_json::from_slice(&body).unwrap();

    // Check structure
    assert!(elevator["warehouse"].is_object());
    assert!(elevator["config"].is_object());
    assert!(elevator["cabins"].is_array());

    // Check default config
    assert_eq!(elevator["config"]["cabin_count"], 3);
    assert_eq!(elevator["config"]["transfer_duration_secs"], 5);

    // Check cabins are initialized
    let cabins = elevator["cabins"].as_array().unwrap();
    assert_eq!(cabins.len(), 3);
    for cabin in cabins {
        assert_eq!(cabin["state"], "available");
    }
}

#[tokio::test]
async fn test_transfer_success() {
    let (app, state) = common::create_test_app_with_state();

    // Create station on Mars via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add some inventory to station for transfer
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut planet.status {
            station.inventory.insert("test_goods".to_string(), 100);
        }
    }

    // Perform transfer as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "test_goods", "quantity": 10}]
    });

    let start = Instant::now();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    // Check result structure
    assert!(result["cabin_id"].is_number());
    assert_eq!(result["duration_secs"], 5);

    // Verify it actually blocked (should take at least 5 seconds)
    assert!(
        elapsed.as_secs() >= 4,
        "Transfer should block for ~5 seconds, but took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_transfer_not_connected() {
    let app = common::create_test_app();

    // Try to transfer on Venus which is uninhabited
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "test", "quantity": 1}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-2/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Uninhabited planets return 404 (not found in settlements)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_transfer_no_cabin_available() {
    let (app, state) = common::create_test_app_with_state();

    // Create station on Mars via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Start 3 transfers concurrently (default cabin count is 3)
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "test_goods", "quantity": 1}]
    });

    // Add inventory for the transfers
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut planet.status {
            station.inventory.insert("test_goods".to_string(), 1000);
        }
    }

    let app1 = app.clone();
    let app2 = app.clone();
    let app3 = app.clone();
    let app4 = app.clone();

    let req_body = transfer_request.to_string();

    // Spawn 3 concurrent transfers
    let handle1 = tokio::spawn({
        let body = req_body.clone();
        async move {
            app1.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                    .header("Content-Type", "application/json")
                    .header("Authorization", player_auth(ALPHA_TOKEN))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
        }
    });

    let handle2 = tokio::spawn({
        let body = req_body.clone();
        async move {
            app2.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                    .header("Content-Type", "application/json")
                    .header("Authorization", player_auth(ALPHA_TOKEN))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
        }
    });

    let handle3 = tokio::spawn({
        let body = req_body.clone();
        async move {
            app3.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                    .header("Content-Type", "application/json")
                    .header("Authorization", player_auth(ALPHA_TOKEN))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
        }
    });

    // Wait a bit for all transfers to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 4th transfer should fail with no cabin available
    let response = app4
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Should return 503 when no cabins available"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error: Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"].as_str().unwrap().contains("No cabin available"));

    // Wait for other transfers to complete
    let _ = handle1.await;
    let _ = handle2.await;
    let _ = handle3.await;
}

#[tokio::test]
async fn test_cabin_states_after_transfer() {
    let (app, state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add inventory for transfer
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut planet.status {
            station.inventory.insert("test_goods".to_string(), 100);
        }
    }

    // Perform a transfer as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "test_goods", "quantity": 10}]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    // Check elevator status after transfer
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settlements/Sol/Sol-4/space-elevator")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let elevator: Value = serde_json::from_slice(&body).unwrap();

    let cabins = elevator["cabins"].as_array().unwrap();

    if result["success"].as_bool().unwrap() {
        // All cabins should be available after successful transfer
        let available_count = cabins
            .iter()
            .filter(|c| c["state"] == "available")
            .count();
        assert_eq!(available_count, 3, "All cabins should be available after successful transfer");
    } else {
        // One cabin should be under repair after failed transfer
        let repair_count = cabins
            .iter()
            .filter(|c| c["state"] == "under_repair")
            .count();
        assert_eq!(repair_count, 1, "One cabin should be under repair after failed transfer");
    }
}

#[tokio::test]
async fn test_transfer_insufficient_stock() {
    let (app, _state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Try to transfer goods that don't exist
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "iron_ore", "quantity": 100}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error: Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"].as_str().unwrap().contains("Insufficient stock"));
}

#[tokio::test]
async fn test_transfer_success_moves_inventory() {
    let (app, state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add inventory to station
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut planet.status {
            station.inventory.insert("iron_ore".to_string(), 100);
        }
    }

    // Transfer to surface as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "iron_ore", "quantity": 50}]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    // Check inventories based on success/failure
    let state = state.galaxy.read().await;
    let system = state.systems.get("Sol").unwrap();
    let planet = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();

    if let PlanetStatus::Connected { station, space_elevator, .. } = &planet.status {
        let station_stock = station.inventory.get("iron_ore").copied().unwrap_or(0);
        let warehouse_stock = space_elevator.warehouse.inventory.get("iron_ore").copied().unwrap_or(0);

        if result["success"].as_bool().unwrap() {
            // Success: 50 moved from station to warehouse
            assert_eq!(station_stock, 50, "Station should have 50 remaining");
            assert_eq!(warehouse_stock, 50, "Warehouse should have 50");
        } else {
            // Failure: goods returned to station
            assert_eq!(station_stock, 100, "Station should have all 100 back");
            assert_eq!(warehouse_stock, 0, "Warehouse should have 0");
        }
    } else {
        panic!("Planet should be connected");
    }
}

#[tokio::test]
async fn test_transfer_failure_reverts_inventory() {
    let (app, state) = common::create_test_app_with_state();

    // Create station with high failure rate config via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Set high failure rate and add inventory
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, space_elevator, .. } = &mut planet.status {
            station.inventory.insert("iron_ore".to_string(), 100);
            // Set 100% failure rate for deterministic test
            space_elevator.config.failure_rate = 10.0; // Very high = guaranteed failure
        }
    }

    // Transfer to surface (will fail)
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "iron_ore", "quantity": 50}]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    // With 100% failure rate, transfer should fail
    assert_eq!(result["success"].as_bool().unwrap(), false);
    assert!(result["failure_reason"].as_str().is_some());

    // Check inventory was reverted
    let state = state.galaxy.read().await;
    let system = state.systems.get("Sol").unwrap();
    let planet = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();

    if let PlanetStatus::Connected { station, space_elevator, .. } = &planet.status {
        let station_stock = station.inventory.get("iron_ore").copied().unwrap_or(0);
        let warehouse_stock = space_elevator.warehouse.inventory.get("iron_ore").copied().unwrap_or(0);

        assert_eq!(station_stock, 100, "Station should have all 100 back after failed transfer");
        assert_eq!(warehouse_stock, 0, "Warehouse should have 0 after failed transfer");
    } else {
        panic!("Planet should be connected");
    }
}

#[tokio::test]
async fn test_transfer_to_orbit_moves_inventory() {
    let (app, state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add inventory to warehouse
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { space_elevator, .. } = &mut planet.status {
            space_elevator.warehouse.inventory.insert("food".to_string(), 200);
            // Set 0% failure rate for deterministic success
            space_elevator.config.failure_rate = 0.0;
        }
    }

    // Transfer to orbit as station owner
    let transfer_request = json!({
        "direction": "to_orbit",
        "items": [{"good_name": "food", "quantity": 75}]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    // With 0% failure rate, transfer should succeed
    assert_eq!(result["success"].as_bool().unwrap(), true);

    // Check inventory moved correctly
    let state = state.galaxy.read().await;
    let system = state.systems.get("Sol").unwrap();
    let planet = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();

    if let PlanetStatus::Connected { station, space_elevator, .. } = &planet.status {
        let station_stock = station.inventory.get("food").copied().unwrap_or(0);
        let warehouse_stock = space_elevator.warehouse.inventory.get("food").copied().unwrap_or(0);

        assert_eq!(warehouse_stock, 125, "Warehouse should have 125 remaining");
        assert_eq!(station_stock, 75, "Station should have 75");
    } else {
        panic!("Planet should be connected");
    }
}

#[tokio::test]
async fn test_transfer_multiple_items() {
    let (app, state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add multiple items to station inventory
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, space_elevator, .. } = &mut planet.status {
            station.inventory.insert("iron_ore".to_string(), 50);
            station.inventory.insert("copper_ore".to_string(), 30);
            station.inventory.insert("gold_ore".to_string(), 10);
            // Set 0% failure rate for deterministic success
            space_elevator.config.failure_rate = 0.0;
        }
    }

    // Transfer multiple items in one trip as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [
            {"good_name": "iron_ore", "quantity": 20},
            {"good_name": "copper_ore", "quantity": 15},
            {"good_name": "gold_ore", "quantity": 5}
        ]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(result["success"].as_bool().unwrap(), true);
    assert_eq!(result["total_quantity"].as_u64().unwrap(), 40); // 20 + 15 + 5
    assert_eq!(result["items"].as_array().unwrap().len(), 3);

    // Check all inventories
    let state = state.galaxy.read().await;
    let system = state.systems.get("Sol").unwrap();
    let planet = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();

    if let PlanetStatus::Connected { station, space_elevator, .. } = &planet.status {
        // Station should have remaining stock
        assert_eq!(station.inventory.get("iron_ore").copied().unwrap_or(0), 30);
        assert_eq!(station.inventory.get("copper_ore").copied().unwrap_or(0), 15);
        assert_eq!(station.inventory.get("gold_ore").copied().unwrap_or(0), 5);

        // Warehouse should have transferred stock
        assert_eq!(space_elevator.warehouse.inventory.get("iron_ore").copied().unwrap_or(0), 20);
        assert_eq!(space_elevator.warehouse.inventory.get("copper_ore").copied().unwrap_or(0), 15);
        assert_eq!(space_elevator.warehouse.inventory.get("gold_ore").copied().unwrap_or(0), 5);
    } else {
        panic!("Planet should be connected");
    }
}

#[tokio::test]
async fn test_transfer_exceeds_capacity() {
    let (app, state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Add inventory exceeding cabin capacity (default is 100)
    {
        let mut state = state.galaxy.write().await;
        let system = state.systems.get_mut("Sol").unwrap();
        let planet = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut planet.status {
            station.inventory.insert("iron_ore".to_string(), 200);
        }
    }

    // Try to transfer more than cabin capacity as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": [{"good_name": "iron_ore", "quantity": 150}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error: Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"].as_str().unwrap().contains("exceeds cabin capacity"));
}

#[tokio::test]
async fn test_transfer_empty_items() {
    let (app, _state) = common::create_test_app_with_state();

    // Create station via admin route
    let station_request = json!({
        "name": "Orbital Station Alpha",
        "owner_id": "alpha-team"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/settlements/Sol/Sol-4/station")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(station_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Try to transfer with empty items array as station owner
    let transfer_request = json!({
        "direction": "to_surface",
        "items": []
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settlements/Sol/Sol-4/space-elevator/transfer")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(transfer_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {}",
        response.status()
    );
}
