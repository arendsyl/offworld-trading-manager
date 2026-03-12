use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::market::MarketState;
use crate::models::{ConstructionProject, Coordinates, MassDriverConnection, PlanetStatus, Player, Ship, TradeRequest};
use crate::models::System;
use crate::pulsar::PulsarManager;

#[derive(Clone)]
pub struct AppState {
    pub galaxy: Arc<RwLock<GalaxyState>>,
    pub players: Arc<RwLock<HashMap<String, Player>>>,
    pub ships: Arc<RwLock<HashMap<Uuid, Ship>>>,
    pub projects: Arc<RwLock<HashMap<Uuid, ConstructionProject>>>,
    pub trade_requests: Arc<RwLock<HashMap<Uuid, TradeRequest>>>,
    pub market: Arc<RwLock<MarketState>>,
    pub pulsar: Option<Arc<PulsarManager>>,
    pub config: Arc<AppConfig>,
    pub http_client: reqwest::Client,
    pub biscuit_root: Arc<biscuit_auth::KeyPair>,
    pub s3: Option<Arc<s3::Bucket>>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GalaxyState {
    pub systems: HashMap<String, System>,
    #[serde(skip)]
    pub connections: HashMap<Uuid, MassDriverConnection>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SeedData {
    pub systems: HashMap<String, System>,
    #[serde(default)]
    pub players: Vec<Player>,
}

impl GalaxyState {
    pub fn new() -> Self {
        Self {
            systems: HashMap::new(),
            connections: HashMap::new(),
        }
    }

    pub fn find_planet_info(&self, planet_id: &str) -> Option<(String, Coordinates, f64, String)> {
        for (system_name, system) in &self.systems {
            for planet in &system.planets {
                if planet.id == planet_id {
                    if let PlanetStatus::Connected { ref station, .. } = planet.status {
                        return Some((
                            system_name.clone(),
                            system.coordinates.clone(),
                            planet.distance_ua,
                            station.owner_id.clone(),
                        ));
                    }
                }
            }
        }
        None
    }

    pub fn find_planet_status(&self, planet_id: &str) -> Option<(String, Coordinates, f64, &PlanetStatus)> {
        for (system_name, system) in &self.systems {
            for planet in &system.planets {
                if planet.id == planet_id {
                    return Some((
                        system_name.clone(),
                        system.coordinates.clone(),
                        planet.distance_ua,
                        &planet.status,
                    ));
                }
            }
        }
        None
    }

    pub fn resolve_planet_owner(&self, system_name: &str, planet_id: &str) -> Option<String> {
        let system = self.systems.get(system_name)?;
        let planet = system.planets.iter().find(|p| p.id == planet_id)?;
        match &planet.status {
            PlanetStatus::Connected { station, .. } => Some(station.owner_id.clone()),
            _ => None,
        }
    }
}

/// Load GalaxyState from a JSON file
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<GalaxyState, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut state: GalaxyState = serde_json::from_str(&content)?;

    // Initialize space elevator cabins for all connected planets
    for system in state.systems.values_mut() {
        for planet in &mut system.planets {
            if let PlanetStatus::Connected { space_elevator, .. } = &mut planet.status {
                space_elevator.ensure_cabins_initialized();
            }
        }
    }

    Ok(state)
}

pub fn load_seed_data<P: AsRef<Path>>(path: P) -> Result<SeedData, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let seed: SeedData = serde_json::from_str(&content)?;
    Ok(seed)
}

pub fn load_players_from_seed<P: AsRef<Path>>(path: P) -> Result<HashMap<String, Player>, Box<dyn std::error::Error>> {
    let seed = load_seed_data(path)?;
    let mut players = HashMap::new();
    for mut player in seed.players {
        player.initial_credits = player.credits;
        players.insert(player.id.clone(), player);
    }
    Ok(players)
}

pub fn create_galaxy_state() -> Arc<RwLock<GalaxyState>> {
    Arc::new(RwLock::new(GalaxyState::new()))
}

pub fn create_galaxy_state_from_file<P: AsRef<Path>>(path: P) -> Result<Arc<RwLock<GalaxyState>>, Box<dyn std::error::Error>> {
    let state = load_from_file(path)?;
    Ok(Arc::new(RwLock::new(state)))
}

pub fn create_app_state() -> AppState {
    AppState {
        galaxy: create_galaxy_state(),
        players: Arc::new(RwLock::new(HashMap::new())),
        ships: Arc::new(RwLock::new(HashMap::new())),
        projects: Arc::new(RwLock::new(HashMap::new())),
        trade_requests: Arc::new(RwLock::new(HashMap::new())),
        market: Arc::new(RwLock::new(MarketState::new(1024))),
        pulsar: None,
        config: Arc::new(AppConfig::default()),
        http_client: reqwest::Client::new(),
        biscuit_root: Arc::new(biscuit_auth::KeyPair::new()),
        s3: None,
    }
}

pub fn create_app_state_from_file<P: AsRef<Path>>(path: P) -> Result<AppState, Box<dyn std::error::Error>> {
    let path_ref = path.as_ref();
    let galaxy = create_galaxy_state_from_file(path_ref)?;
    let players = load_players_from_seed(path_ref).unwrap_or_default();
    Ok(AppState {
        galaxy,
        players: Arc::new(RwLock::new(players)),
        ships: Arc::new(RwLock::new(HashMap::new())),
        projects: Arc::new(RwLock::new(HashMap::new())),
        trade_requests: Arc::new(RwLock::new(HashMap::new())),
        market: Arc::new(RwLock::new(MarketState::new(1024))),
        pulsar: None,
        config: Arc::new(AppConfig::default()),
        http_client: reqwest::Client::new(),
        biscuit_root: Arc::new(biscuit_auth::KeyPair::new()),
        s3: None,
    })
}
