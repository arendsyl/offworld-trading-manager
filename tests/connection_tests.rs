use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use offworld_trading_manager::models::{MassDriver, PlanetStatus};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::ADMIN_TOKEN;

fn admin_auth() -> String {
    format!("Bearer {}", ADMIN_TOKEN)
}

/// Helper: create a station on Mars (Sol-4) which is settled
async fn setup_mars_station(app: &axum::Router) {
    let station_request = json!({
        "name": "Mars Orbital",
        "owner_id": "player-1"
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
}

/// Helper: create connection between Earth (Sol-3) and Mars (Sol-4)
async fn create_earth_mars_connection(app: &axum::Router) -> Value {
    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Sol-4"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn test_create_connection() {
    let (app, _state) = common::create_test_app_with_state();

    // Setup Mars with a station
    setup_mars_station(&app).await;

    let connection = create_earth_mars_connection(&app).await;

    assert_eq!(connection["system"], "Sol");
    assert_eq!(connection["from_planet"], "Sol-3");
    assert_eq!(connection["to_planet"], "Sol-4");
    assert_eq!(connection["status"], "pending");
    assert!(connection["id"].is_string());
}

#[tokio::test]
async fn test_create_connection_different_systems() {
    let app = common::create_test_app();

    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Proxima Centauri-1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_connection_not_connected() {
    let app = common::create_test_app();

    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Sol-4"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_connection_same_planet() {
    let app = common::create_test_app();

    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Sol-3"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_accept_connection() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    let update = json!({ "action": "accept" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["status"], "active");
}

#[tokio::test]
async fn test_reject_connection() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    let update = json!({ "action": "reject" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["status"], "closed");
}

#[tokio::test]
async fn test_close_connection() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    // Accept first
    let update = json!({ "action": "accept" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Close
    let update = json!({ "action": "close" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["status"], "closed");
}

#[tokio::test]
async fn test_max_channels_exceeded() {
    let (app, state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    // Set Earth's mass driver to 1 channel
    {
        let mut galaxy = state.galaxy.write().await;
        let system = galaxy.systems.get_mut("Sol").unwrap();
        let earth = system.planets.iter_mut().find(|p| p.id == "Sol-3").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut earth.status {
            station.mass_driver = Some(MassDriver::new(1));
        }
    }

    // Create first connection (should succeed)
    let conn1 = create_earth_mars_connection(&app).await;
    let id1 = conn1["id"].as_str().unwrap();

    // Accept it so it becomes Active and occupies a channel
    let update = json!({ "action": "accept" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id1))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Try to create second connection (should fail - no channels)
    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Sol-4"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_list_connections() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    // Create a connection
    create_earth_mars_connection(&app).await;

    // List all connections
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/connections")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let connections: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections.len(), 1);
}

#[tokio::test]
async fn test_list_connections_filter_system() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    create_earth_mars_connection(&app).await;

    // Filter by system
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/connections?system=Sol")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let connections: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections.len(), 1);

    // Filter by nonexistent system
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/connections?system=NonExistent")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let connections: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections.len(), 0);
}

#[tokio::test]
async fn test_list_connections_filter_planet() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    create_earth_mars_connection(&app).await;

    // Filter by from_planet
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/connections?planet=Sol-3")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let connections: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections.len(), 1);

    // Filter by to_planet
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/connections?planet=Sol-4")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let connections: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections.len(), 1);
}

#[tokio::test]
async fn test_get_connection() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/admin/connections/{}", id))
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let fetched: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(fetched["id"], id);
}

#[tokio::test]
async fn test_delete_connection() {
    let (app, state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    // Accept first
    let update = json!({ "action": "accept" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Delete
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!("/admin/connections/{}", id))
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify it's gone
    let galaxy = state.galaxy.read().await;
    assert!(!galaxy.connections.contains_key(&id.parse().unwrap()));
}

#[tokio::test]
async fn test_close_frees_channels() {
    let (app, state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;

    // Set Earth to 1 channel
    {
        let mut galaxy = state.galaxy.write().await;
        let system = galaxy.systems.get_mut("Sol").unwrap();
        let earth = system.planets.iter_mut().find(|p| p.id == "Sol-3").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut earth.status {
            station.mass_driver = Some(MassDriver::new(1));
        }
    }

    // Create and accept connection
    let conn = create_earth_mars_connection(&app).await;
    let id = conn["id"].as_str().unwrap();

    let update = json!({ "action": "accept" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Close the connection
    let update = json!({ "action": "close" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be able to create a new connection now
    let request = json!({
        "system": "Sol",
        "from_planet": "Sol-3",
        "to_planet": "Sol-4"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/connections")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_accept_already_active() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    // Accept
    let update = json!({ "action": "accept" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Try to accept again
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_close_pending_fails() {
    let (app, _state) = common::create_test_app_with_state();
    setup_mars_station(&app).await;
    let connection = create_earth_mars_connection(&app).await;
    let id = connection["id"].as_str().unwrap();

    // Try to close a pending connection
    let update = json!({ "action": "close" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(&format!("/admin/connections/{}", id))
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
