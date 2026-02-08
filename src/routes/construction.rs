use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::construction_lifecycle::{spawn_construction_project, spawn_upgrade_project};
use crate::error::{AppError, ConstructionError};
use crate::models::{
    ConstructionProject, FoundSettlementRequest, InstallStationRequest, PlanetStatus,
    ProjectStatus, ProjectType, StationUpgradeType, UpgradeElevatorRequest,
    UpgradeStationRequest,
};
use crate::ship_lifecycle::calculate_travel_time;
use crate::state::AppState;

pub fn player_construction_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects))
        .route("/install-station", post(install_station))
        .route("/found-settlement", post(found_settlement))
        .route("/upgrade-station", post(upgrade_station))
        .route("/upgrade-elevator", post(upgrade_elevator))
        .route("/{project_id}", get(get_project))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[instrument(skip(state, auth))]
async fn install_station(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<InstallStationRequest>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    if body.source_planet_id == body.target_planet_id {
        return Err(ConstructionError::SamePlanet.into());
    }

    let fee = state.config.construction.station_install_fee;
    let required_goods = state.config.construction.station_install_goods.clone();
    let build_secs = state.config.construction.build_base_secs;

    // Validate source is Connected + player owns it, validate target is Settled + not Connected
    let (source_coords, source_au, source_system, target_coords, target_au, target_system) = {
        let galaxy = state.galaxy.read().await;

        // Validate source
        let (src_sys, src_coords, src_au, src_status) = galaxy
            .find_planet_status(&body.source_planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(body.source_planet_id.clone()))?;
        match src_status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotSourceStationOwner.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(body.source_planet_id.clone()).into()),
        }

        // Validate target
        let (tgt_sys, tgt_coords, tgt_au, tgt_status) = galaxy
            .find_planet_status(&body.target_planet_id)
            .ok_or_else(|| ConstructionError::TargetPlanetNotFound(body.target_planet_id.clone()))?;
        match tgt_status {
            PlanetStatus::Settled { .. } => {}
            PlanetStatus::Connected { .. } => {
                return Err(ConstructionError::TargetAlreadyConnected(body.target_planet_id.clone()).into());
            }
            PlanetStatus::Uninhabited => {
                return Err(ConstructionError::TargetNotSettled(body.target_planet_id.clone()).into());
            }
        }

        (src_coords, src_au, src_sys, tgt_coords, tgt_au, tgt_sys)
    };

    // Validate credits
    {
        let players = state.players.read().await;
        let player = players
            .get(&auth.0.id)
            .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
        if player.credits < fee as i64 {
            return Err(ConstructionError::InsufficientCredits {
                needed: fee,
                available: player.credits,
            }
            .into());
        }
    }

    // Validate + deduct goods from source station, deduct credits atomically
    {
        let mut galaxy = state.galaxy.write().await;
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if planet.id == body.source_planet_id {
                    if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                        for (good, &qty) in &required_goods {
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
                        // Deduct goods
                        for (good, &qty) in &required_goods {
                            let entry = station.inventory.entry(good.clone()).or_insert(0);
                            *entry -= qty;
                            if *entry == 0 {
                                station.inventory.remove(good);
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduct credits
    {
        let mut players = state.players.write().await;
        if let Some(player) = players.get_mut(&auth.0.id) {
            player.credits -= fee as i64;
        }
    }

    // Calculate travel time
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

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::InstallStation,
        source_planet_id: body.source_planet_id.clone(),
        target_planet_id: body.target_planet_id.clone(),
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::InTransit,
        created_at: now,
        completion_at,
        station_name: Some(body.station_name),
        settlement_name: None,
    };

    let project_id = project.id;

    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    // Get callback URL
    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&auth.0.id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

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

#[instrument(skip(state, auth))]
async fn found_settlement(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<FoundSettlementRequest>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    if body.source_planet_id == body.target_planet_id {
        return Err(ConstructionError::SamePlanet.into());
    }

    let fee = state.config.construction.settlement_found_fee;
    let required_goods = state.config.construction.settlement_found_goods.clone();
    let build_secs = state.config.construction.build_base_secs;

    // Validate source is Connected + player owns it, validate target is Uninhabited
    let (source_coords, source_au, source_system, target_coords, target_au, target_system) = {
        let galaxy = state.galaxy.read().await;

        let (src_sys, src_coords, src_au, src_status) = galaxy
            .find_planet_status(&body.source_planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(body.source_planet_id.clone()))?;
        match src_status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotSourceStationOwner.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(body.source_planet_id.clone()).into()),
        }

        let (tgt_sys, tgt_coords, tgt_au, tgt_status) = galaxy
            .find_planet_status(&body.target_planet_id)
            .ok_or_else(|| ConstructionError::TargetPlanetNotFound(body.target_planet_id.clone()))?;
        match tgt_status {
            PlanetStatus::Uninhabited => {}
            _ => return Err(ConstructionError::TargetNotUninhabited(body.target_planet_id.clone()).into()),
        }

        (src_coords, src_au, src_sys, tgt_coords, tgt_au, tgt_sys)
    };

    // Validate credits
    {
        let players = state.players.read().await;
        let player = players
            .get(&auth.0.id)
            .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
        if player.credits < fee as i64 {
            return Err(ConstructionError::InsufficientCredits {
                needed: fee,
                available: player.credits,
            }
            .into());
        }
    }

    // Combine required_goods + extra_goods for total deduction from source
    let mut total_goods = required_goods.clone();
    for (good, &qty) in &body.extra_goods {
        *total_goods.entry(good.clone()).or_insert(0) += qty;
    }

    // Validate + deduct all goods from source station
    {
        let mut galaxy = state.galaxy.write().await;
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if planet.id == body.source_planet_id {
                    if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                        for (good, &qty) in &total_goods {
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
                        for (good, &qty) in &total_goods {
                            let entry = station.inventory.entry(good.clone()).or_insert(0);
                            *entry -= qty;
                            if *entry == 0 {
                                station.inventory.remove(good);
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduct credits
    {
        let mut players = state.players.write().await;
        if let Some(player) = players.get_mut(&auth.0.id) {
            player.credits -= fee as i64;
        }
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

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::FoundSettlement,
        source_planet_id: body.source_planet_id.clone(),
        target_planet_id: body.target_planet_id.clone(),
        fee,
        goods_consumed: required_goods,
        extra_goods: body.extra_goods,
        status: ProjectStatus::InTransit,
        created_at: now,
        completion_at,
        station_name: Some(body.station_name),
        settlement_name: Some(body.settlement_name),
    };

    let project_id = project.id;

    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&auth.0.id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

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

#[instrument(skip(state, auth))]
async fn upgrade_station(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<UpgradeStationRequest>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    let (fee, required_goods) = match body.upgrade_type {
        StationUpgradeType::DockingBays => (
            state.config.construction.upgrade_docking_bay_fee,
            state.config.construction.upgrade_docking_bay_goods.clone(),
        ),
        StationUpgradeType::MassDriverChannels => (
            state.config.construction.upgrade_mass_driver_fee,
            state.config.construction.upgrade_mass_driver_goods.clone(),
        ),
        StationUpgradeType::Storage => (
            state.config.construction.upgrade_storage_fee,
            state.config.construction.upgrade_storage_goods.clone(),
        ),
    };
    let build_secs = state.config.construction.upgrade_build_secs;

    let project_type = match body.upgrade_type {
        StationUpgradeType::DockingBays => ProjectType::UpgradeDockingBays,
        StationUpgradeType::MassDriverChannels => ProjectType::UpgradeMassDriverChannels,
        StationUpgradeType::Storage => ProjectType::UpgradeStorage,
    };

    // Validate planet is Connected + player owns station + has mass driver for that upgrade
    {
        let galaxy = state.galaxy.read().await;
        let (_sys, _coords, _au, status) = galaxy
            .find_planet_status(&body.planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(body.planet_id.clone()))?;
        match status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotTargetStationOwner.into());
                }
                if body.upgrade_type == StationUpgradeType::MassDriverChannels
                    && station.mass_driver.is_none()
                {
                    return Err(ConstructionError::NoMassDriver.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(body.planet_id.clone()).into()),
        }
    }

    // Validate credits
    {
        let players = state.players.read().await;
        let player = players
            .get(&auth.0.id)
            .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
        if player.credits < fee as i64 {
            return Err(ConstructionError::InsufficientCredits {
                needed: fee,
                available: player.credits,
            }
            .into());
        }
    }

    // Validate + deduct goods from station inventory
    {
        let mut galaxy = state.galaxy.write().await;
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if planet.id == body.planet_id {
                    if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                        for (good, &qty) in &required_goods {
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
                        for (good, &qty) in &required_goods {
                            let entry = station.inventory.entry(good.clone()).or_insert(0);
                            *entry -= qty;
                            if *entry == 0 {
                                station.inventory.remove(good);
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduct credits
    {
        let mut players = state.players.write().await;
        if let Some(player) = players.get_mut(&auth.0.id) {
            player.credits -= fee as i64;
        }
    }

    let now = now_ms();
    let completion_at = now + (build_secs * 1000.0) as u64;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type,
        source_planet_id: body.planet_id.clone(),
        target_planet_id: body.planet_id.clone(),
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::Building,
        created_at: now,
        completion_at,
        station_name: None,
        settlement_name: None,
    };

    let project_id = project.id;

    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&auth.0.id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

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

#[instrument(skip(state, auth))]
async fn upgrade_elevator(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<UpgradeElevatorRequest>,
) -> Result<(StatusCode, Json<ConstructionProject>), AppError> {
    let fee = state.config.construction.upgrade_cabin_fee;
    let required_goods = state.config.construction.upgrade_cabin_goods.clone();
    let build_secs = state.config.construction.upgrade_build_secs;

    // Validate planet is Connected + player owns station
    {
        let galaxy = state.galaxy.read().await;
        let (_sys, _coords, _au, status) = galaxy
            .find_planet_status(&body.planet_id)
            .ok_or_else(|| ConstructionError::SourceStationNotFound(body.planet_id.clone()))?;
        match status {
            PlanetStatus::Connected { station, .. } => {
                if station.owner_id != auth.0.id {
                    return Err(ConstructionError::NotTargetStationOwner.into());
                }
            }
            _ => return Err(ConstructionError::SourceStationNotFound(body.planet_id.clone()).into()),
        }
    }

    // Validate credits
    {
        let players = state.players.read().await;
        let player = players
            .get(&auth.0.id)
            .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
        if player.credits < fee as i64 {
            return Err(ConstructionError::InsufficientCredits {
                needed: fee,
                available: player.credits,
            }
            .into());
        }
    }

    // Validate + deduct goods from warehouse (space elevator's warehouse)
    {
        let mut galaxy = state.galaxy.write().await;
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if planet.id == body.planet_id {
                    if let PlanetStatus::Connected { ref mut space_elevator, .. } = planet.status {
                        for (good, &qty) in &required_goods {
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
                        for (good, &qty) in &required_goods {
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
                    }
                }
            }
        }
    }

    // Deduct credits
    {
        let mut players = state.players.write().await;
        if let Some(player) = players.get_mut(&auth.0.id) {
            player.credits -= fee as i64;
        }
    }

    let now = now_ms();
    let completion_at = now + (build_secs * 1000.0) as u64;

    let project = ConstructionProject {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        project_type: ProjectType::UpgradeElevatorCabins,
        source_planet_id: body.planet_id.clone(),
        target_planet_id: body.planet_id.clone(),
        fee,
        goods_consumed: required_goods,
        extra_goods: HashMap::new(),
        status: ProjectStatus::Building,
        created_at: now,
        completion_at,
        station_name: None,
        settlement_name: None,
    };

    let project_id = project.id;

    {
        let mut projects = state.projects.write().await;
        projects.insert(project_id, project.clone());
    }

    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&auth.0.id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

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

#[instrument(skip(state, auth))]
async fn list_projects(
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

#[instrument(skip(state, auth))]
async fn get_project(
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
