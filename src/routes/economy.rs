use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;

use crate::economy::models::{Demographics, EconomyState};
use crate::error::AppError;
use crate::models::PlanetStatus;
use crate::state::AppState;

pub fn player_economy_router() -> Router<AppState> {
    Router::new()
        .route("/{system_name}/{planet_id}/economy", get(get_economy))
        .route(
            "/{system_name}/{planet_id}/economy/prices",
            get(get_economy_prices),
        )
        .route(
            "/{system_name}/{planet_id}/economy/demographics",
            get(get_economy_demographics),
        )
        .route(
            "/{system_name}/{planet_id}/economy/flows",
            get(get_economy_flows),
        )
        .route(
            "/{system_name}/{planet_id}/economy/stocks",
            get(get_economy_stocks),
        )
}

#[derive(Serialize, ToSchema)]
pub struct DemographicsResponse {
    pub demographics: Demographics,
    pub wage: f64,
    pub unemployment: f64,
    pub national_income: f64,
    pub labor_alloc: HashMap<String, f64>,
    pub infrastructure: f64,
    pub carrying_capacity: f64,
    pub crowding: f64,
}

#[derive(Serialize, ToSchema)]
pub struct FlowsResponse {
    pub production: HashMap<String, f64>,
    pub consumption: HashMap<String, f64>,
    pub investment: HashMap<String, f64>,
    pub infra_investment: HashMap<String, f64>,
    pub intermediate: HashMap<String, f64>,
    pub available_supply: HashMap<String, f64>,
    pub demand: HashMap<String, f64>,
}

fn find_economy<'a>(
    galaxy: &'a crate::state::GalaxyState,
    system_name: &str,
    planet_id: &str,
) -> Result<&'a EconomyState, AppError> {
    let system = galaxy
        .systems
        .get(system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.to_string()))?;

    let planet = system
        .planets
        .iter()
        .find(|p| p.id == planet_id)
        .ok_or_else(|| AppError::PlanetNotFound(planet_id.to_string()))?;

    match &planet.status {
        PlanetStatus::Settled { settlement } => Ok(&settlement.economy),
        PlanetStatus::Connected { settlement, .. } => Ok(&settlement.economy),
        PlanetStatus::Uninhabited => Err(AppError::SettlementNotFound(planet_id.to_string())),
    }
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/economy",
    tag = "economy",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Full economy state", body = EconomyState),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_economy(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<EconomyState>, AppError> {
    let galaxy = state.galaxy.read().await;
    let economy = find_economy(&galaxy, &system_name, &planet_id)?;
    Ok(Json(economy.clone()))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/economy/prices",
    tag = "economy",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Current prices", body = HashMap<String, f64>),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_economy_prices(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<HashMap<String, f64>>, AppError> {
    let galaxy = state.galaxy.read().await;
    let economy = find_economy(&galaxy, &system_name, &planet_id)?;
    Ok(Json(economy.prices.clone()))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/economy/demographics",
    tag = "economy",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Demographics and labor stats", body = DemographicsResponse),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_economy_demographics(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<DemographicsResponse>, AppError> {
    let galaxy = state.galaxy.read().await;
    let economy = find_economy(&galaxy, &system_name, &planet_id)?;
    Ok(Json(DemographicsResponse {
        demographics: economy.demographics.clone(),
        wage: economy.wage,
        unemployment: economy.unemployment,
        national_income: economy.national_income,
        labor_alloc: economy.labor_alloc.clone(),
        infrastructure: economy.infrastructure,
        carrying_capacity: economy.last_carrying_capacity,
        crowding: economy.last_crowding,
    }))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/economy/flows",
    tag = "economy",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Last-tick flow snapshots", body = FlowsResponse),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_economy_flows(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<FlowsResponse>, AppError> {
    let galaxy = state.galaxy.read().await;
    let economy = find_economy(&galaxy, &system_name, &planet_id)?;
    Ok(Json(FlowsResponse {
        production: economy.last_production.clone(),
        consumption: economy.last_consumption.clone(),
        investment: economy.last_investment.clone(),
        infra_investment: economy.last_infra_investment.clone(),
        intermediate: economy.last_intermediate.clone(),
        available_supply: economy.last_available_supply.clone(),
        demand: economy.last_demand.clone(),
    }))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/economy/stocks",
    tag = "economy",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Physical goods stockpile", body = HashMap<String, f64>),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_economy_stocks(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<HashMap<String, f64>>, AppError> {
    let galaxy = state.galaxy.read().await;
    let economy = find_economy(&galaxy, &system_name, &planet_id)?;
    Ok(Json(economy.stocks.clone()))
}
