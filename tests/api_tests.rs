use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::{create_test_app, ADMIN_TOKEN};

fn admin_auth() -> String {
    format!("Bearer {}", ADMIN_TOKEN)
}

#[tokio::test]
async fn test_list_systems() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let systems: Vec<Value> = serde_json::from_slice(&body).unwrap();

    assert_eq!(systems.len(), 3);
}

#[tokio::test]
async fn test_create_system() {
    let app = create_test_app();

    let new_system = json!({
        "name": "Alpha Centauri",
        "coordinates": { "x": 4.37, "y": 1.0, "z": 0.5 },
        "star_type": "binary_system"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/systems")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(new_system.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let system: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(system["name"], "Alpha Centauri");
    assert!(system["coordinates"].is_object());
}

#[tokio::test]
async fn test_get_system() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems/Sol")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_system_not_found() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems/00000000-0000-0000-0000-000000000000")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_system() {
    let app = common::create_test_app();

    let update = json!({
        "name": "Updated System Name"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/systems/Sol")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let system: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(system["name"], "Updated System Name");
}

#[tokio::test]
async fn test_delete_system() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/systems/Sol")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_list_planets() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems/Sol/planets")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let planets: Vec<Value> = serde_json::from_slice(&body).unwrap();

    assert_eq!(planets.len(), 6);
}

#[tokio::test]
async fn test_create_planet() {
    let app = common::create_test_app();

    let new_planet = json!({
        "name": "Uranus",
        "position": 7,
        "distance_ua": 19.2,
        "planet_type": {
            "category": "gas_giant",
            "gas_type": "ice_giant"
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/systems/Sol/planets")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(new_planet.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let planet: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(planet["name"], "Uranus");
    assert_eq!(planet["id"], "Sol-7");
}

#[tokio::test]
async fn test_get_planet() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems/Sol/planets/Sol-3")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let planet: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(planet["name"], "Earth");
}

#[tokio::test]
async fn test_update_planet() {
    let app = common::create_test_app();

    let update = json!({
        "name": "Terra"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/systems/Sol/planets/Sol-3")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let planet: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(planet["name"], "Terra");
}

#[tokio::test]
async fn test_delete_planet() {
    let app = common::create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/systems/Sol/planets/Sol-1")
                .header("Authorization", admin_auth())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_create_planet_conflict() {
    let app = common::create_test_app();

    let duplicate_planet = json!({
        "name": "Another Earth",
        "position": 3,
        "distance_ua": 1.0,
        "planet_type": {
            "category": "telluric",
            "climate": "temperate"
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/systems/Sol/planets")
                .header("Content-Type", "application/json")
                .header("Authorization", admin_auth())
                .body(Body::from(duplicate_planet.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_admin_requires_auth() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_rejects_player_token() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/systems")
                .header("Authorization", format!("Bearer {}", common::ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
