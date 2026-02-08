use axum::{
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::models::Player;
use crate::state::AppState;

fn extract_bearer_token(parts: &Parts) -> Result<&str, AppError> {
    let header = parts
        .headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)
}

#[derive(Debug)]
pub struct AuthenticatedPlayer(pub Player);

impl FromRequestParts<AppState> for AuthenticatedPlayer {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = extract_bearer_token(parts)?;

        let players = state.players.read().await;
        let player = players
            .values()
            .find(|p| p.api_key == token)
            .cloned()
            .ok_or(AppError::Unauthorized)?;

        Ok(AuthenticatedPlayer(player))
    }
}

#[derive(Debug)]
pub struct AdminAuth;

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = extract_bearer_token(parts)?;

        if token != state.config.admin.token {
            return Err(AppError::Unauthorized);
        }

        Ok(AdminAuth)
    }
}

pub async fn admin_auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    if token != state.config.admin.token {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(request).await)
}

pub async fn player_auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let players = state.players.read().await;
    let _player = players
        .values()
        .find(|p| p.api_key == token)
        .ok_or(AppError::Unauthorized)?;

    drop(players);

    Ok(next.run(request).await)
}
