use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::{debug, info, instrument};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::error::{AppError, MassDriverError};
use crate::models::{
    ConnectionAction, ConnectionStatus, CreateConnectionRequest, MassDriverConnection,
    NotifyMessage, PlanetStatus, UpdateConnectionRequest,
};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn admin_connections_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_connections).post(create_connection))
        .route(
            "/{id}",
            get(get_connection)
                .put(update_connection)
                .delete(delete_connection),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ConnectionFilter {
    pub system: Option<String>,
    pub planet: Option<String>,
}

fn get_planet_connections<'a>(
    connections: impl Iterator<Item = &'a MassDriverConnection>,
    planet_id: &str,
) -> Vec<&'a MassDriverConnection> {
    connections
        .filter(|c| c.from_planet == planet_id || c.to_planet == planet_id)
        .collect()
}

#[utoipa::path(
    get,
    path = "/admin/connections",
    tag = "connections",
    params(ConnectionFilter),
    responses(
        (status = 200, description = "List of mass driver connections", body = Vec<MassDriverConnection>),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn list_connections(
    State(state): State<AppState>,
    Query(filter): Query<ConnectionFilter>,
) -> Json<Vec<MassDriverConnection>> {
    debug!(filter = ?filter, "Listing connections");
    let galaxy = state.galaxy.read().await;

    let connections: Vec<MassDriverConnection> = galaxy
        .connections
        .values()
        .filter(|c| {
            let system_match = filter
                .system
                .as_ref()
                .map_or(true, |s| &c.system == s);
            let planet_match = filter
                .planet
                .as_ref()
                .map_or(true, |p| &c.from_planet == p || &c.to_planet == p);
            system_match && planet_match
        })
        .cloned()
        .collect();

    debug!(count = connections.len(), "Returning connections");
    Json(connections)
}

#[utoipa::path(
    get,
    path = "/admin/connections/{id}",
    tag = "connections",
    params(
        ("id" = Uuid, Path, description = "Connection ID"),
    ),
    responses(
        (status = 200, description = "Connection details", body = MassDriverConnection),
        (status = 404, description = "Connection not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn get_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MassDriverConnection>, AppError> {
    debug!("Getting connection");
    let galaxy = state.galaxy.read().await;

    let connection = galaxy
        .connections
        .get(&id)
        .ok_or_else(|| MassDriverError::ConnectionNotFound(id.to_string()))?;

    Ok(Json(connection.clone()))
}

#[utoipa::path(
    post,
    path = "/admin/connections",
    tag = "connections",
    request_body = CreateConnectionRequest,
    responses(
        (status = 201, description = "Connection created", body = MassDriverConnection),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "System or planet not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state, payload))]
pub async fn create_connection(
    State(state): State<AppState>,
    Json(payload): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<MassDriverConnection>), AppError> {
    validate_input(&payload)?;
    debug!(
        system = %payload.system,
        from = %payload.from_planet,
        to = %payload.to_planet,
        "Creating connection"
    );

    if payload.from_planet == payload.to_planet {
        return Err(MassDriverError::SameStation.into());
    }

    let mut galaxy = state.galaxy.write().await;

    // Validate both planets are in the same system and Connected
    let system = galaxy
        .systems
        .get(&payload.system)
        .ok_or_else(|| AppError::SystemNotFound(payload.system.clone()))?;

    let from_planet = system
        .planets
        .iter()
        .find(|p| p.id == payload.from_planet)
        .ok_or_else(|| AppError::PlanetNotFound(payload.from_planet.clone()))?;

    let to_planet = system
        .planets
        .iter()
        .find(|p| p.id == payload.to_planet)
        .ok_or_else(|| AppError::PlanetNotFound(payload.to_planet.clone()))?;

    // Check from_planet is Connected and has available channel
    let from_mass_driver = match &from_planet.status {
        PlanetStatus::Connected { station, .. } => station
            .mass_driver
            .as_ref()
            .ok_or_else(|| MassDriverError::PlanetNotConnected(payload.from_planet.clone()))?,
        _ => return Err(MassDriverError::PlanetNotConnected(payload.from_planet.clone()).into()),
    };

    // Check to_planet is Connected
    match &to_planet.status {
        PlanetStatus::Connected { .. } => {}
        _ => return Err(MassDriverError::PlanetNotConnected(payload.to_planet.clone()).into()),
    };

    // Check from has available channel
    let from_connections: Vec<&MassDriverConnection> =
        get_planet_connections(galaxy.connections.values(), &payload.from_planet);
    if !from_mass_driver.has_available_channel(&from_connections) {
        return Err(MassDriverError::NoChannelAvailable(payload.from_planet.clone()).into());
    }

    let connection = MassDriverConnection {
        id: Uuid::new_v4(),
        system: payload.system.clone(),
        from_planet: payload.from_planet.clone(),
        to_planet: payload.to_planet.clone(),
        status: ConnectionStatus::Pending,
    };

    galaxy
        .connections
        .insert(connection.id, connection.clone());

    info!(
        connection_id = %connection.id,
        from = %connection.from_planet,
        to = %connection.to_planet,
        "Connection created (pending)"
    );

    // Notify to_planet owner via Pulsar if available
    if let Some(ref pulsar) = state.pulsar {
        if let Some(to_owner) = galaxy.resolve_planet_owner(&payload.system, &payload.to_planet) {
            pulsar
                .send_notification(
                    &to_owner,
                    &NotifyMessage::ConnectionRequest {
                        connection_id: connection.id,
                        from_planet: payload.from_planet.clone(),
                    },
                )
                .await;
        }
    }

    Ok((StatusCode::CREATED, Json(connection)))
}

#[utoipa::path(
    put,
    path = "/admin/connections/{id}",
    tag = "connections",
    params(
        ("id" = Uuid, Path, description = "Connection ID"),
    ),
    request_body = UpdateConnectionRequest,
    responses(
        (status = 200, description = "Connection updated", body = MassDriverConnection),
        (status = 400, description = "Invalid connection state"),
        (status = 404, description = "Connection not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state, payload))]
pub async fn update_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateConnectionRequest>,
) -> Result<Json<MassDriverConnection>, AppError> {
    debug!(action = ?payload.action, "Updating connection");
    let mut galaxy = state.galaxy.write().await;

    let connection = galaxy
        .connections
        .get(&id)
        .ok_or_else(|| MassDriverError::ConnectionNotFound(id.to_string()))?
        .clone();

    match payload.action {
        ConnectionAction::Accept => {
            if connection.status != ConnectionStatus::Pending {
                return Err(MassDriverError::InvalidConnectionState.into());
            }

            // Check to_planet has available channel
            let system = galaxy
                .systems
                .get(&connection.system)
                .ok_or_else(|| AppError::SystemNotFound(connection.system.clone()))?;

            let to_planet = system
                .planets
                .iter()
                .find(|p| p.id == connection.to_planet)
                .ok_or_else(|| AppError::PlanetNotFound(connection.to_planet.clone()))?;

            let to_mass_driver = match &to_planet.status {
                PlanetStatus::Connected { station, .. } => station
                    .mass_driver
                    .as_ref()
                    .ok_or_else(|| {
                        MassDriverError::PlanetNotConnected(connection.to_planet.clone())
                    })?,
                _ => {
                    return Err(
                        MassDriverError::PlanetNotConnected(connection.to_planet.clone()).into(),
                    )
                }
            };

            // Also re-check from_planet has channel (it could have filled up)
            let from_planet = system
                .planets
                .iter()
                .find(|p| p.id == connection.from_planet)
                .ok_or_else(|| AppError::PlanetNotFound(connection.from_planet.clone()))?;

            let from_mass_driver = match &from_planet.status {
                PlanetStatus::Connected { station, .. } => station
                    .mass_driver
                    .as_ref()
                    .ok_or_else(|| {
                        MassDriverError::PlanetNotConnected(connection.from_planet.clone())
                    })?,
                _ => {
                    return Err(
                        MassDriverError::PlanetNotConnected(connection.from_planet.clone()).into(),
                    )
                }
            };

            let to_connections: Vec<&MassDriverConnection> =
                get_planet_connections(galaxy.connections.values(), &connection.to_planet);
            if !to_mass_driver.has_available_channel(&to_connections) {
                return Err(
                    MassDriverError::NoChannelAvailable(connection.to_planet.clone()).into()
                );
            }

            let from_connections: Vec<&MassDriverConnection> =
                get_planet_connections(galaxy.connections.values(), &connection.from_planet);
            if !from_mass_driver.has_available_channel(&from_connections) {
                return Err(
                    MassDriverError::NoChannelAvailable(connection.from_planet.clone()).into()
                );
            }

            let conn = galaxy.connections.get_mut(&id)
                .ok_or_else(|| AppError::Internal(format!("connection {id} disappeared")))?;
            conn.status = ConnectionStatus::Active;
            let updated = conn.clone();

            info!(connection_id = %id, "Connection accepted");

            if let Some(ref pulsar) = state.pulsar {
                if let Some(from_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.from_planet) {
                    pulsar
                        .send_notification(
                            &from_owner,
                            &NotifyMessage::ConnectionAccepted {
                                connection_id: id,
                            },
                        )
                        .await;
                }
            }

            Ok(Json(updated))
        }
        ConnectionAction::Reject => {
            if connection.status != ConnectionStatus::Pending {
                return Err(MassDriverError::InvalidConnectionState.into());
            }

            let conn = galaxy.connections.get_mut(&id)
                .ok_or_else(|| AppError::Internal(format!("connection {id} disappeared")))?;
            conn.status = ConnectionStatus::Closed;
            let updated = conn.clone();

            info!(connection_id = %id, "Connection rejected");

            if let Some(ref pulsar) = state.pulsar {
                if let Some(from_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.from_planet) {
                    pulsar
                        .send_notification(
                            &from_owner,
                            &NotifyMessage::ConnectionRejected {
                                connection_id: id,
                            },
                        )
                        .await;
                }
            }

            Ok(Json(updated))
        }
        ConnectionAction::Close => {
            if connection.status != ConnectionStatus::Active {
                return Err(MassDriverError::InvalidConnectionState.into());
            }

            let conn = galaxy.connections.get_mut(&id)
                .ok_or_else(|| AppError::Internal(format!("connection {id} disappeared")))?;
            conn.status = ConnectionStatus::Closed;
            let updated = conn.clone();

            info!(connection_id = %id, "Connection closed");

            if let Some(ref pulsar) = state.pulsar {
                // Notify both parties
                if let Some(from_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.from_planet) {
                    pulsar
                        .send_notification(
                            &from_owner,
                            &NotifyMessage::ConnectionClosed {
                                connection_id: id,
                                closed_by: "admin".to_string(),
                            },
                        )
                        .await;
                }
                if let Some(to_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.to_planet) {
                    pulsar
                        .send_notification(
                            &to_owner,
                            &NotifyMessage::ConnectionClosed {
                                connection_id: id,
                                closed_by: "admin".to_string(),
                            },
                        )
                        .await;
                }
            }

            Ok(Json(updated))
        }
    }
}

#[utoipa::path(
    delete,
    path = "/admin/connections/{id}",
    tag = "connections",
    params(
        ("id" = Uuid, Path, description = "Connection ID"),
    ),
    responses(
        (status = 204, description = "Connection deleted"),
        (status = 404, description = "Connection not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn delete_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    debug!("Deleting connection");
    let mut galaxy = state.galaxy.write().await;

    let connection = galaxy
        .connections
        .get(&id)
        .ok_or_else(|| MassDriverError::ConnectionNotFound(id.to_string()))?
        .clone();

    // If active, treat as close first
    if connection.status == ConnectionStatus::Active {
        if let Some(ref pulsar) = state.pulsar {
            if let Some(from_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.from_planet) {
                pulsar
                    .send_notification(
                        &from_owner,
                        &NotifyMessage::ConnectionClosed {
                            connection_id: id,
                            closed_by: "delete".to_string(),
                        },
                    )
                    .await;
            }
            if let Some(to_owner) = galaxy.resolve_planet_owner(&connection.system, &connection.to_planet) {
                pulsar
                    .send_notification(
                        &to_owner,
                        &NotifyMessage::ConnectionClosed {
                            connection_id: id,
                            closed_by: "delete".to_string(),
                        },
                    )
                    .await;
            }
        }
    }

    galaxy.connections.remove(&id);
    info!(connection_id = %id, "Connection deleted");

    Ok(StatusCode::NO_CONTENT)
}
