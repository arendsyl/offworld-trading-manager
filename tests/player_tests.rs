mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::{ADMIN_TOKEN, ALPHA_TOKEN, BETA_TOKEN};
use offworld_trading_manager::models::{
    CreatePlayerRequest, PlayerPublic, PlayerSelfView, UpdatePlayerRequest,
};

fn admin_auth() -> String {
    format!("Bearer {}", ADMIN_TOKEN)
}

fn player_auth(token: &str) -> String {
    format!("Bearer {}", token)
}

// --- Admin routes ---

#[tokio::test]
async fn test_list_players() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/players")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let players: Vec<PlayerPublic> = serde_json::from_slice(&body).unwrap();
    assert_eq!(players.len(), 3);
}

#[tokio::test]
async fn test_get_player_admin() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/players/alpha-team")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerPublic = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "alpha-team");
    assert_eq!(player.name, "Alpha Trading Co.");
    assert_eq!(player.credits, 100000);
}

// --- Player routes ---

#[tokio::test]
async fn test_get_player() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/players/alpha-team")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerSelfView = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "alpha-team");
    assert_eq!(player.name, "Alpha Trading Co.");
    assert_eq!(player.credits, 100000);
    assert_eq!(player.api_key, ALPHA_TOKEN);
}

#[tokio::test]
async fn test_get_player_not_found() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/players/nonexistent")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Player trying to access another player's data gets Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_update_callback_url() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
        name: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/players/alpha-team")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerSelfView = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "alpha-team");
    assert_eq!(player.callback_url, "http://localhost:9999/new-webhook");
}

#[tokio::test]
async fn test_update_requires_auth() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
        name: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/players/alpha-team")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_invalid_api_key() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
        name: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/players/alpha-team")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer wrong-key")
                .body(Body::from(serde_json::to_string(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_forbidden_wrong_player() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
        name: None,
    };

    // Beta trying to update Alpha's profile
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/players/alpha-team")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// --- New tests ---

#[tokio::test]
async fn test_create_player() {
    let app = common::create_test_app();

    let create = CreatePlayerRequest {
        id: "new-player".to_string(),
        name: "New Player Inc.".to_string(),
        credits: Some(5000),
        callback_url: Some("http://example.com/webhook".to_string()),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/players")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerSelfView = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "new-player");
    assert_eq!(player.name, "New Player Inc.");
    assert_eq!(player.credits, 5000);
    assert_eq!(player.callback_url, "http://example.com/webhook");
    assert!(!player.api_key.is_empty());
    assert!(!player.pulsar_biscuit.is_empty());
}

#[tokio::test]
async fn test_create_player_duplicate() {
    let app = common::create_test_app();

    let create = CreatePlayerRequest {
        id: "alpha-team".to_string(),
        name: "Duplicate".to_string(),
        credits: None,
        callback_url: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/players")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(serde_json::to_string(&create).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_delete_player() {
    let (app, state) = common::create_test_app_with_state();

    // First verify the player exists
    {
        let players = state.players.read().await;
        assert!(players.contains_key("alpha-team"));
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/players/alpha-team")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify the player is gone
    {
        let players = state.players.read().await;
        assert!(!players.contains_key("alpha-team"));
    }
}

#[tokio::test]
async fn test_delete_player_not_found() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/players/nonexistent")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_player_cascading_ships() {
    let (app, state) = common::create_test_app_with_state();

    // Insert a ship owned by alpha-team
    {
        use offworld_trading_manager::models::{Ship, ShipStatus};
        let mut ships = state.ships.write().await;
        let ship = Ship {
            id: uuid::Uuid::new_v4(),
            owner_id: "alpha-team".to_string(),
            origin_planet_id: "planet-a".to_string(),
            destination_planet_id: "planet-b".to_string(),
            cargo: std::collections::HashMap::new(),
            status: ShipStatus::InTransit,
            trade_id: None,
            trucking_id: None,
            fee: None,
            created_at: 0,
            arrival_at: None,
            operation_complete_at: None,
            estimated_arrival_at: None,
            callback_url: String::new(),
        };
        ships.insert(ship.id, ship);
    }

    // Verify ship exists
    {
        let ships = state.ships.read().await;
        assert_eq!(ships.values().filter(|s| s.owner_id == "alpha-team").count(), 1);
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/players/alpha-team")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify ship is gone
    {
        let ships = state.ships.read().await;
        assert_eq!(ships.values().filter(|s| s.owner_id == "alpha-team").count(), 0);
    }
}

#[tokio::test]
async fn test_rename_player() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: None,
        name: Some("Alpha Renamed".to_string()),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/players/alpha-team")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerSelfView = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.name, "Alpha Renamed");
}

#[tokio::test]
async fn test_regenerate_token() {
    let (app, state) = common::create_test_app_with_state();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/players/alpha-team/regenerate-token")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let player: PlayerSelfView = serde_json::from_slice(&body).unwrap();
    assert_ne!(player.api_key, ALPHA_TOKEN);
    assert!(!player.api_key.is_empty());

    // Verify the new token is persisted in state
    {
        let players = state.players.read().await;
        let stored = players.get("alpha-team").unwrap();
        assert_eq!(stored.api_key, player.api_key);
    }

    // Verify old token no longer works
    let app2 = common::build_router(state);
    let response2 = app2
        .oneshot(
            Request::builder()
                .uri("/players/alpha-team")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::UNAUTHORIZED);
}
