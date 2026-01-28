use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use tracing::warn;

use crate::models::SpaceElevatorError;

#[derive(Debug, Clone, Error)]
pub enum MassDriverError {
    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),
    #[error("No channel available on station at planet: {0}")]
    NoChannelAvailable(String),
    #[error("Planets are in different systems")]
    DifferentSystems,
    #[error("Planet is not connected: {0}")]
    PlanetNotConnected(String),
    #[error("Invalid connection state for this action")]
    InvalidConnectionState,
    #[error("Packet too large: {size} > {max} max")]
    PacketTooLarge { size: u64, max: u64 },
    #[error("Insufficient inventory: {good_name} (need {requested}, have {available})")]
    InsufficientInventory {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Connection is not active")]
    ConnectionNotActive,
    #[error("Cannot create connection to the same station")]
    SameStation,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("System not found: {0}")]
    SystemNotFound(String),

    #[error("Planet not found: {0}")]
    PlanetNotFound(String),

    #[error("Settlement not found on planet: {0}")]
    SettlementNotFound(String),

    #[error("Station not found on planet: {0}")]
    StationNotFound(String),

    #[error("Planet already exists: {0}")]
    PlanetAlreadyExists(String),

    #[error("Planet is uninhabited, settlement required: {0}")]
    SettlementRequired(String),

    #[error("Planet is not connected (no station/space elevator): {0}")]
    NotConnected(String),

    #[error("{0}")]
    SpaceElevator(#[from] SpaceElevatorError),

    #[error("{0}")]
    MassDriver(#[from] MassDriverError),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        warn!(error = %self, "Request failed with error");
        let (status, message) = match &self {
            AppError::SystemNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::PlanetNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::SettlementNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::StationNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::PlanetAlreadyExists(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::SettlementRequired(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::NotConnected(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::SpaceElevator(e) => {
                let status = match e {
                    SpaceElevatorError::NoCabinAvailable => StatusCode::SERVICE_UNAVAILABLE,
                    SpaceElevatorError::InsufficientStock { .. } => StatusCode::BAD_REQUEST,
                    SpaceElevatorError::ExceedsCapacity { .. } => StatusCode::BAD_REQUEST,
                    SpaceElevatorError::EmptyTransfer => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
            AppError::MassDriver(e) => {
                let status = match e {
                    MassDriverError::ConnectionNotFound(_) => StatusCode::NOT_FOUND,
                    MassDriverError::NoChannelAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
                    MassDriverError::DifferentSystems => StatusCode::BAD_REQUEST,
                    MassDriverError::PlanetNotConnected(_) => StatusCode::BAD_REQUEST,
                    MassDriverError::InvalidConnectionState => StatusCode::CONFLICT,
                    MassDriverError::PacketTooLarge { .. } => StatusCode::BAD_REQUEST,
                    MassDriverError::InsufficientInventory { .. } => StatusCode::BAD_REQUEST,
                    MassDriverError::ConnectionNotActive => StatusCode::CONFLICT,
                    MassDriverError::SameStation => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}
