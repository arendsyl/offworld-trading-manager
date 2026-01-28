use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{MassDriverConnection, PlanetStatus};
use crate::models::System;
use crate::pulsar::PulsarManager;

#[derive(Clone)]
pub struct AppState {
    pub galaxy: Arc<RwLock<GalaxyState>>,
    pub pulsar: Option<Arc<PulsarManager>>,
    pub config: Arc<AppConfig>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GalaxyState {
    pub systems: HashMap<String, System>,
    #[serde(skip)]
    pub connections: HashMap<Uuid, MassDriverConnection>,
}

impl GalaxyState {
    pub fn new() -> Self {
        Self {
            systems: HashMap::new(),
            connections: HashMap::new(),
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
        pulsar: None,
        config: Arc::new(AppConfig::default()),
    }
}

pub fn create_app_state_from_file<P: AsRef<Path>>(path: P) -> Result<AppState, Box<dyn std::error::Error>> {
    let galaxy = create_galaxy_state_from_file(path)?;
    Ok(AppState {
        galaxy,
        pulsar: None,
        config: Arc::new(AppConfig::default()),
    })
}
