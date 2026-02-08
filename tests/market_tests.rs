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
    Order, OrderBookSummary, OrderSide, OrderStatus, OrderType, PlaceOrderRequest,
};
use offworld_trading_manager::state::AppState;

fn player_auth(token: &str) -> String {
    format!("Bearer {}", token)
}

fn create_market_test_state() -> AppState {
    let seed_path = concat!(env!("CARGO_MANIFEST_DIR"), "/seed.json");
    let mut state = offworld_trading_manager::state::create_app_state_from_file(seed_path)
        .expect("Failed to load test seed data");

    let mut config = AppConfig::default();
    config.ship.au_to_seconds = 0.01;
    config.ship.seconds_per_unit = 0.001;
    config.ship.webhook_timeout_secs = 1;
    config.admin.token = common::ADMIN_TOKEN.to_string();
    state.config = Arc::new(config);

    state
}

// Sol-3 (Earth) owned by alpha-team
// Proxima Centauri-1 (Proxima b) owned by beta-corp

#[tokio::test]
async fn test_place_limit_sell_order() {
    let state = create_market_test_state();
    let app = common::build_router(state.clone());

    let order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 50,
        station_planet_id: "Sol-3".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Order = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.status, OrderStatus::Open);
    assert_eq!(result.good_name, "iron_ore");
    assert_eq!(result.quantity, 50);
    assert_eq!(result.filled_quantity, 0);

    // Verify inventory was deducted from station
    let galaxy = state.galaxy.read().await;
    let sol = galaxy.systems.get("Sol").unwrap();
    let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
    if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
        earth.status
    {
        // Was 5000, should now be 4950
        assert_eq!(station.inventory.get("iron_ore"), Some(&4950));
    }
}

#[tokio::test]
async fn test_place_limit_buy_order_reserves_credits() {
    let state = create_market_test_state();
    let app = common::build_router(state.clone());

    let order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 50,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    // Verify credits were deducted: 100 * 50 = 5000
    let players = state.players.read().await;
    let beta = players.get("beta-corp").unwrap();
    assert_eq!(beta.credits, 75000 - 5000);
}

#[tokio::test]
async fn test_insufficient_credits() {
    let state = create_market_test_state();
    let app = common::build_router(state);

    let order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(100000),
        quantity: 50,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_insufficient_inventory() {
    let state = create_market_test_state();
    let app = common::build_router(state);

    let order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 999999,
        station_planet_id: "Sol-3".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_limit_order_matching() {
    let state = create_market_test_state();

    // Alpha places a sell order
    let sell_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 30,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&sell_order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Beta places a buy order at matching price
    let buy_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 30,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&buy_order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Order = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.status, OrderStatus::Filled);
    assert_eq!(result.filled_quantity, 30);

    // Verify credits transferred: seller gets 30 * 100 = 3000
    let players = state.players.read().await;
    let alpha = players.get("alpha-team").unwrap();
    assert_eq!(alpha.credits, 100000 + 3000);

    let beta = players.get("beta-corp").unwrap();
    // Beta paid 30 * 100 = 3000 reserved upfront
    assert_eq!(beta.credits, 75000 - 3000);

    // A ship should have been spawned
    let ships = state.ships.read().await;
    assert_eq!(ships.len(), 1);
}

#[tokio::test]
async fn test_partial_fill() {
    let state = create_market_test_state();

    // Alpha sells 20 units
    let sell_order = PlaceOrderRequest {
        good_name: "copper_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(50),
        quantity: 20,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::from(serde_json::to_string(&sell_order).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    // Beta buys 50 units (only 20 available)
    let buy_order = PlaceOrderRequest {
        good_name: "copper_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(50),
        quantity: 50,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&buy_order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Order = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.status, OrderStatus::PartiallyFilled);
    assert_eq!(result.filled_quantity, 20);
}

#[tokio::test]
async fn test_cancel_order_returns_credits() {
    let state = create_market_test_state();

    let buy_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(200),
        quantity: 10,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&buy_order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let order: Order = serde_json::from_slice(&body).unwrap();
    let order_id = order.id;

    {
        let players = state.players.read().await;
        let beta = players.get("beta-corp").unwrap();
        assert_eq!(beta.credits, 75000 - 2000);
    }

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/market/orders/{}", order_id))
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let players = state.players.read().await;
    let beta = players.get("beta-corp").unwrap();
    assert_eq!(beta.credits, 75000);
}

#[tokio::test]
async fn test_cancel_order_returns_inventory() {
    let state = create_market_test_state();

    let sell_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 100,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::from(serde_json::to_string(&sell_order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let order: Order = serde_json::from_slice(&body).unwrap();

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

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("DELETE")
            .uri(format!("/market/orders/{}", order.id))
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    let galaxy = state.galaxy.read().await;
    let sol = galaxy.systems.get("Sol").unwrap();
    let earth = sol.planets.iter().find(|p| p.id == "Sol-3").unwrap();
    if let offworld_trading_manager::models::PlanetStatus::Connected { ref station, .. } =
        earth.status
    {
        assert_eq!(station.inventory.get("iron_ore"), Some(&5000));
    }
}

#[tokio::test]
async fn test_order_book_summary() {
    let state = create_market_test_state();

    let sell_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 50,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::from(serde_json::to_string(&sell_order).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    let buy_order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(80),
        quantity: 30,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(BETA_TOKEN))
            .body(Body::from(serde_json::to_string(&buy_order).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/market/book/iron_ore")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let summary: OrderBookSummary = serde_json::from_slice(&body).unwrap();
    assert_eq!(summary.good_name, "iron_ore");
    assert_eq!(summary.asks.len(), 1);
    assert_eq!(summary.asks[0].price, 100);
    assert_eq!(summary.asks[0].total_quantity, 50);
    assert_eq!(summary.bids.len(), 1);
    assert_eq!(summary.bids[0].price, 80);
    assert_eq!(summary.bids[0].total_quantity, 30);
}

#[tokio::test]
async fn test_limit_order_requires_price() {
    let state = create_market_test_state();
    let app = common::build_router(state);

    let order = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: None,
        quantity: 10,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&order).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_prices() {
    let state = create_market_test_state();

    let sell = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 10,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::from(serde_json::to_string(&sell).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    let buy = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 10,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(BETA_TOKEN))
            .body(Body::from(serde_json::to_string(&buy).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/market/prices")
                .header("Authorization", player_auth(ALPHA_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let prices: HashMap<String, u64> = serde_json::from_slice(&body).unwrap();
    assert_eq!(prices.get("iron_ore"), Some(&100));
}

#[tokio::test]
async fn test_market_order_immediate_fill() {
    let state = create_market_test_state();

    let sell = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        price: Some(100),
        quantity: 20,
        station_planet_id: "Sol-3".to_string(),
    };

    let app = common::build_router(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/market/orders")
            .header("Content-Type", "application/json")
            .header("Authorization", player_auth(ALPHA_TOKEN))
            .body(Body::from(serde_json::to_string(&sell).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap();

    let buy = PlaceOrderRequest {
        good_name: "iron_ore".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        price: None,
        quantity: 20,
        station_planet_id: "Proxima Centauri-1".to_string(),
    };

    let app = common::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/market/orders")
                .header("Content-Type", "application/json")
                .header("Authorization", player_auth(BETA_TOKEN))
                .body(Body::from(serde_json::to_string(&buy).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Order = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.status, OrderStatus::Filled);
    assert_eq!(result.filled_quantity, 20);
}
