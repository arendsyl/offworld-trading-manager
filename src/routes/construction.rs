use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::construction_lifecycle::{spawn_construction_project, spawn_upgrade_project};
use crate::error::{AppError, ConstructionError};
use crate::models::{
    ConstructionProject, CreateProjectRequest, PlanetStatus, ProjectStatus, ProjectType,
};
use crate::ship_lifecycle::calculate_travel_time;
use crate::state::AppState;

pub fn player_projects_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects).post(create_project))
        .route("/{project_id}", get(get_project))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

/// Validate goods availability and deduct from station inventory.
fn validate_and_deduct_station_goods(
    galaxy: &mut crate::state::GalaxyState,
    planet_id: &str,
    required_goods: &HashMap<String, u64>,
) -> Result<(), AppError> {
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == planet_id {
                if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                    // Validate
                    for (good, &qty) in required_goods {
                        let available = station.inventory.get(good).copied().unwrap_or(0);
                        if available < qty {
                            return Err(ConstructionError::InsufficientGoods {
                                good_name: good.clone(),
                                requested: qty,
                                available,
                            }
                            .into());
                        }
                    }
                    // Deduct
                    for (good, &qty) in required_goods {
                        let entry = station.inventory.entry(good.clone()).or_insert(0);
                        *entry -= qty;
                        if *entry == 0 {
                            station.inventory.remove(good);
                        }
                    }
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

/// Validate goods availability and deduct from warehouse (space elevator) inventory.
fn validate_and_deduct_warehouse_goods(
    galaxy: &mut crate::state::GalaxyState,
    planet_id: &str,
    required_goods: &HashMap<String, u64>,
) -> Result<(), AppError> {
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == planet_id {
                if let PlanetStatus::Connected {
                    ref mut space_elevator,
                    ..
                } = planet.status
                {
                    // Validate
                    for (good, &qty) in required_goods {
                        let available = space_elevator
                            .warehouse
                            .inventory
                            .get(good)
                            .copied()
                            .unwrap_or(0);
                        if available < qty {
                            return Err(ConstructionError::InsufficientGoods {
                                good_name: good.clone(),
                                requested: qty,
                                available,
                            }
                            .into());
                        }
                    }
                    // Deduct
                    for (good, &qty) in required_goods {
                        let entry = space_elevator
                            .warehouse
                            .inventory
                            .entry(good.clone())
                            .or_insert(0);
                        *entry -= qty;
                        if *entry == 0 {
                            space_elevator.warehouse.inventory.remove(good);
                        }
                    }
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

/// Validate credits and deduct from player.
async fn validate_and_deduct_credits(
    state: &AppState,
    player_id: &str,
    fee: u64,
) -> Result<(), AppError> {
    let mut players = state.players.write().await;
    let player = players
        .get_mut(player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id.to_string()))?;
    if player.credits < fee as i64 {
        return Err(ConstructionError::InsufficientCredits {
            needed: fee,
            available: player.credits,
        }
        .into());
    }
    player.credits -= fee as i64;
    Ok(())
}

/// Get callback URL for a player.
async fn get_callback_url(state: &AppState, player_id: &str) -> String {
    let players = state.players.read().await;
    players
        .get(player_id)
        .map(|p| p.callback_url.clone())
        .unwrap_or_default()
}

#[utoipa::path(
    post,
    path = "/projects",
    tag = "projects",
    security(("api_key" = [])),
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "Project created", body = ConstructionProject),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn create_project(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    match body {
        CreateProjectRequest::InstallStation {
            source_planet_id,
            target_planet_id,
            station_name,
        } => {
            create_install_station(state, auth, source_planet_id, target_planet_id, station_name)
                .await
        }
        CreateProjectRequest::FoundSettlement {
            source_planet_id,
            target_planet_id,
            settlement_name,
            station_name,
            extra_goods,
        } => {
            create_found_settlement(
                state,
                auth,
                source_planet_id,
                target_planet_id,
                settlement_name,
                station_name,
                extra_goods,
            )
            .await
        }
        CreateProjectRequest::UpgradeDockingBays { planet_id } => {
            create_upgrade_station(state, auth, planet_id, ProjectType::UpgradeDockingBays).await
        }
        CreateProjectRequest::UpgradeMassDriverChannels { planet_id } => {
            create_upgrade_station(
                state,
                auth,
                planet_id,
                ProjectType::UpgradeMassDriverChannels,
            )
            .await
        }
        CreateProjectRequest::UpgradeStorage { planet_id } => {
            create_upgrade_station(state, auth, planet_id, ProjectType::UpgradeStorage).await
        }
        CreateProjectRequest::UpgradeElevatorCabins { planet_id } => {
            create_upgrade_elevator(state, auth, planet_id).await
        }
    }
}

async fn create_install_station(
    state: AppState,
    auth: AuthenticatedPlayer,
    source_planet_id: String,
    target_planet_id: String,
    station_name: String,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    if source_planet_id == target_planet_id {
        return Err(ConstructionError::SamePlanet.into());
    }

    let fee = state.config.construction.station_install_fee;
    let required_goods = state.config.construction.station_install_goods.clone();
    let build_secs = state.config.construction.build_base_secs;

    let (source_coords, source_au, source_system, target_coords, target_au, target_system) = {
        let galaxy = state.galaxy.read().await;

        let (src_sys, src_coords, src_au, src_status) = galaxy
            .find_planet_status(&source_planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(source_planet_id.clone()))?;
        match src_status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotSourceStationOwner.into());
                }
            }
            _ => {
                return Err(
                    ConstructionError::SourceStationNotFound(source_planet_id.clone()).into(),
                )
            }
        }

        let (tgt_sys, tgt_coords, tgt_au, tgt_status) = galaxy
            .find_planet_status(&target_planet_id)
            .ok_or_else(|| ConstructionError::TargetPlanetNotFound(target_planet_id.clone()))?;
        match tgt_status {
            PlanetStatus::Settled { .. } => {}
            PlanetStatus::Connected { .. } => {
                return Err(
                    ConstructionError::TargetAlreadyConnected(target_planet_id.clone()).into(),
                );
            }
            PlanetStatus::Uninhabited => {
                return Err(
                    ConstructionError::TargetNotSettled(target_planet_id.clone()).into(),
                );
            }
        }

        (src_coords, src_au, src_sys, tgt_coords, tgt_au, tgt_sys)
    };

    // Validate + deduct credits
    validate_and_deduct_credits(&state, &auth.0.id, fee).await?;

    // Validate + deduct goods from source station
    {
        let mut galaxy = state.galaxy.write().await;
        validate_and_deduct_station_goods(&mut galaxy, &source_planet_id, &required_goods)?;
    }

    let same_system = source_system == target_system;
    let transit_secs = calculate_travel_time(
        &source_coords,
        source_au,
        &target_coords,
        target_au,
        same_system,
        &state.config.trucking,
    );

    let now = now_ms();
    let completion_at = now + ((transit_secs + build_secs) * 1000.0) as u64;

    let callback_url = get_callback_url(&state, &auth.0.id).await;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::InstallStation,
        source_planet_id,
        target_planet_id,
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::InTransit,
        created_at: now,
        completion_at,
        station_name: Some(station_name),
        settlement_name: None,
        transit_ends_at: Some(now + (transit_secs * 1000.0) as u64),
        callback_url: callback_url.clone(),
    };

    let project_id = project.id;
    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    spawn_construction_project(
        state.projects.clone(),
        state.galaxy.clone(),
        state.config.clone(),
        project_id,
        transit_secs,
        build_secs,
        callback_url,
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(project)))
}

async fn create_found_settlement(
    state: AppState,
    auth: AuthenticatedPlayer,
    source_planet_id: String,
    target_planet_id: String,
    settlement_name: String,
    station_name: String,
    extra_goods: HashMap<String, u64>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    if source_planet_id == target_planet_id {
        return Err(ConstructionError::SamePlanet.into());
    }

    let fee = state.config.construction.settlement_found_fee;
    let required_goods = state.config.construction.settlement_found_goods.clone();
    let build_secs = state.config.construction.build_base_secs;

    let (source_coords, source_au, source_system, target_coords, target_au, target_system) = {
        let galaxy = state.galaxy.read().await;

        let (src_sys, src_coords, src_au, src_status) = galaxy
            .find_planet_status(&source_planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(source_planet_id.clone()))?;
        match src_status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotSourceStationOwner.into());
                }
            }
            _ => {
                return Err(
                    ConstructionError::SourceStationNotFound(source_planet_id.clone()).into(),
                )
            }
        }

        let (tgt_sys, tgt_coords, tgt_au, tgt_status) = galaxy
            .find_planet_status(&target_planet_id)
            .ok_or_else(|| ConstructionError::TargetPlanetNotFound(target_planet_id.clone()))?;
        match tgt_status {
            PlanetStatus::Uninhabited => {}
            _ => {
                return Err(
                    ConstructionError::TargetNotUninhabited(target_planet_id.clone()).into(),
                );
            }
        }

        (src_coords, src_au, src_sys, tgt_coords, tgt_au, tgt_sys)
    };

    // Validate + deduct credits
    validate_and_deduct_credits(&state, &auth.0.id, fee).await?;

    // Combine required_goods + extra_goods for total deduction
    let mut total_goods = required_goods.clone();
    for (good, &qty) in &extra_goods {
        *total_goods.entry(good.clone()).or_insert(0) += qty;
    }

    // Validate + deduct all goods from source station
    {
        let mut galaxy = state.galaxy.write().await;
        validate_and_deduct_station_goods(&mut galaxy, &source_planet_id, &total_goods)?;
    }

    let same_system = source_system == target_system;
    let transit_secs = calculate_travel_time(
        &source_coords,
        source_au,
        &target_coords,
        target_au,
        same_system,
        &state.config.trucking,
    );

    let now = now_ms();
    let completion_at = now + ((transit_secs + build_secs) * 1000.0) as u64;

    let callback_url = get_callback_url(&state, &auth.0.id).await;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::FoundSettlement,
        source_planet_id,
        target_planet_id,
        fee,
        goods_consumed: required_goods,
        extra_goods,
        status: ProjectStatus::InTransit,
        created_at: now,
        completion_at,
        station_name: Some(station_name),
        settlement_name: Some(settlement_name),
        transit_ends_at: Some(now + (transit_secs * 1000.0) as u64),
        callback_url: callback_url.clone(),
    };

    let project_id = project.id;
    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    spawn_construction_project(
        state.projects.clone(),
        state.galaxy.clone(),
        state.config.clone(),
        project_id,
        transit_secs,
        build_secs,
        callback_url,
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(project)))
}

async fn create_upgrade_station(
    state: AppState,
    auth: AuthenticatedPlayer,
    planet_id: String,
    project_type: ProjectType,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    let (fee, required_goods) = match project_type {
        ProjectType::UpgradeDockingBays => (
            state.config.construction.upgrade_docking_bay_fee,
            state.config.construction.upgrade_docking_bay_goods.clone(),
        ),
        ProjectType::UpgradeMassDriverChannels => (
            state.config.construction.upgrade_mass_driver_fee,
            state.config.construction.upgrade_mass_driver_goods.clone(),
        ),
        ProjectType::UpgradeStorage => (
            state.config.construction.upgrade_storage_fee,
            state.config.construction.upgrade_storage_goods.clone(),
        ),
        _ => unreachable!(),
    };
    let build_secs = state.config.construction.upgrade_build_secs;

    // Validate planet is Connected + player owns station
    {
        let galaxy = state.galaxy.read().await;
        let (_sys, _coords, _au, status) = galaxy
            .find_planet_status(&planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(planet_id.clone()))?;
        match status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotTargetStationOwner.into());
                }
                if project_type == ProjectType::UpgradeMassDriverChannels
                    && station.mass_driver.is_none()
                {
                    return Err(ConstructionError::NoMassDriver.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(planet_id.clone()).into()),
        }
    }

    // Validate + deduct credits
    validate_and_deduct_credits(&state, &auth.0.id, fee).await?;

    // Validate + deduct goods from station inventory
    {
        let mut galaxy = state.galaxy.write().await;
        validate_and_deduct_station_goods(&mut galaxy, &planet_id, &required_goods)?;
    }

    let now = now_ms();
    let completion_at = now + (build_secs * 1000.0) as u64;

    let callback_url = get_callback_url(&state, &auth.0.id).await;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type,
        source_planet_id: planet_id.clone(),
        target_planet_id: planet_id,
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::Building,
        created_at: now,
        completion_at,
        station_name: None,
        settlement_name: None,
        transit_ends_at: None,
        callback_url: callback_url.clone(),
    };

    let project_id = project.id;
    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    spawn_upgrade_project(
        state.projects.clone(),
        state.galaxy.clone(),
        state.config.clone(),
        project_id,
        build_secs,
        callback_url,
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(project)))
}

async fn create_upgrade_elevator(
    state: AppState,
    auth: AuthenticatedPlayer,
    planet_id: String,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    let fee = state.config.construction.upgrade_cabin_fee;
    let required_goods = state.config.construction.upgrade_cabin_goods.clone();
    let build_secs = state.config.construction.upgrade_build_secs;

    // Validate planet is Connected + player owns station
    {
        let galaxy = state.galaxy.read().await;
        let (_sys, _coords, _au, status) = galaxy
            .find_planet_status(&planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(planet_id.clone()))?;
        match status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotTargetStationOwner.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(planet_id.clone()).into()),
        }
    }

    // Validate + deduct credits
    validate_and_deduct_credits(&state, &auth.0.id, fee).await?;

    // Validate + deduct goods from warehouse (space elevator's warehouse)
    {
        let mut galaxy = state.galaxy.write().await;
        validate_and_deduct_warehouse_goods(&mut galaxy, &planet_id, &required_goods)?;
    }

    let now = now_ms();
    let completion_at = now + (build_secs * 1000.0) as u64;

    let callback_url = get_callback_url(&state, &auth.0.id).await;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::UpgradeElevatorCabins,
        source_planet_id: planet_id.clone(),
        target_planet_id: planet_id,
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::Building,
        created_at: now,
        completion_at,
        station_name: None,
        settlement_name: None,
        transit_ends_at: None,
        callback_url: callback_url.clone(),
    };

    let project_id = project.id;
    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    spawn_upgrade_project(
        state.projects.clone(),
        state.galaxy.clone(),
        state.config.clone(),
        project_id,
        build_secs,
        callback_url,
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(project)))
}

#[utoipa::path(
    get,
    path = "/projects",
    tag = "projects",
    security(("api_key" = [])),
    responses(
        (status = 200, description = "List of construction projects", body = Vec<ConstructionProject>),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn list_projects(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
) -> Json<Vec<ConstructionProject>> {
    let projects = state.projects.read().await;
    let result: Vec<ConstructionProject> = projects
        .values()
        .filter(|p| p.owner_id == auth.0.id)
        .cloned()
        .collect();
    Json(result)
}

#[utoipa::path(
    get,
    path = "/projects/{project_id}",
    tag = "projects",
    security(("api_key" = [])),
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Construction project details", body = ConstructionProject),
        (status = 404, description = "Project not found"),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn get_project(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(project_id): Path<Uuid>,
) -> Result<Json<ConstructionProject>, AppError> {
    let projects = state.projects.read().await;
    let project = projects
        .get(&project_id)
        .ok_or_else(|| ConstructionError::ProjectNotFound(project_id.to_string()))?;
    if project.owner_id != auth.0.id {
        return Err(ConstructionError::ProjectNotFound(project_id.to_string()).into());
    }
    Ok(Json(project.clone()))
}
