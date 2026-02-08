mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::{ADMIN_TOKEN, ALPHA_TOKEN, BETA_TOKEN};
use offworld_trading_manager::models::{PlayerPublic, UpdatePlayerRequest};

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
    let player: PlayerPublic = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "alpha-team");
    assert_eq!(player.name, "Alpha Trading Co.");
    assert_eq!(player.credits, 100000);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_callback_url() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
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
    let player: PlayerPublic = serde_json::from_slice(&body).unwrap();
    assert_eq!(player.id, "alpha-team");
}

#[tokio::test]
async fn test_update_requires_auth() {
    let app = common::create_test_app();

    let update = UpdatePlayerRequest {
        callback_url: Some("http://localhost:9999/new-webhook".to_string()),
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
