use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::economy::EconomyState;
use crate::models::{
    Cabin, ConstructionProject, ConstructionWebhookPayload, MassDriver,
    PlanetStatus, ProjectStatus, ProjectType, Settlement, SpaceElevator,
    SpaceElevatorConfig, Station, Warehouse,
};

pub fn spawn_construction_project(
    projects: Arc<RwLock<HashMap<Uuid, ConstructionProject>>>,
    galaxy: Arc<RwLock<crate::state::GalaxyState>>,
    config: Arc<AppConfig>,
    project_id: Uuid,
    transit_secs: f64,
    build_secs: f64,
    callback_url: String,
    http_client: reqwest::Client,
) {
    tokio::spawn(async move {
        // Phase 1: Transit
        tokio::time::sleep(Duration::from_secs_f64(transit_secs)).await;

        let project_snapshot = {
            let mut projects = projects.write().await;
            if let Some(project) = projects.get_mut(&project_id) {
                project.status = ProjectStatus::Building;
                project.clone()
            } else {
                return;
            }
        };

        // Phase 2: Building
        tokio::time::sleep(Duration::from_secs_f64(build_secs)).await;

        // Apply galaxy mutation based on project type
        match project_snapshot.project_type {
            ProjectType::InstallStation => {
                apply_install_station(&galaxy, &config, &project_snapshot).await;
            }
            ProjectType::FoundSettlement => {
                apply_found_settlement(&galaxy, &config, &project_snapshot).await;
            }
            _ => {}
        }

        // Mark complete
        {
            let mut projects = projects.write().await;
            if let Some(project) = projects.get_mut(&project_id) {
                project.status = ProjectStatus::Complete;
            }
        }

        // Send webhook
        let payload = ConstructionWebhookPayload::ConstructionComplete {
            project_id,
            project_type: project_snapshot.project_type.clone(),
            target_planet_id: project_snapshot.target_planet_id.clone(),
        };
        send_construction_webhook(
            &http_client,
            &callback_url,
            &payload,
            config.ship.webhook_timeout_secs,
            project_id,
        )
        .await;

        info!(project_id = %project_id, "Construction project completed");
    });
}

pub fn spawn_upgrade_project(
    projects: Arc<RwLock<HashMap<Uuid, ConstructionProject>>>,
    galaxy: Arc<RwLock<crate::state::GalaxyState>>,
    config: Arc<AppConfig>,
    project_id: Uuid,
    build_secs: f64,
    callback_url: String,
    http_client: reqwest::Client,
) {
    tokio::spawn(async move {
        // Single phase: building
        tokio::time::sleep(Duration::from_secs_f64(build_secs)).await;

        let project_snapshot = {
            let projects_read = projects.read().await;
            match projects_read.get(&project_id) {
                Some(p) => p.clone(),
                None => return,
            }
        };

        // Apply upgrade mutation
        match project_snapshot.project_type {
            ProjectType::UpgradeDockingBays => {
                let mut galaxy = galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == project_snapshot.target_planet_id {
                            if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                                station.docking_bays += 1;
                            }
                        }
                    }
                }
            }
            ProjectType::UpgradeMassDriverChannels => {
                let mut galaxy = galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == project_snapshot.target_planet_id {
                            if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                                if let Some(ref mut md) = station.mass_driver {
                                    md.max_channels += 1;
                                }
                            }
                        }
                    }
                }
            }
            ProjectType::UpgradeStorage => {
                let mut galaxy = galaxy.write().await;
                let increment = config.construction.storage_increment;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == project_snapshot.target_planet_id {
                            if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                                station.max_storage = station.max_storage.saturating_add(increment);
                            }
                        }
                    }
                }
            }
            ProjectType::UpgradeElevatorCabins => {
                let mut galaxy = galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == project_snapshot.target_planet_id {
                            if let PlanetStatus::Connected { ref mut space_elevator, .. } = planet.status {
                                space_elevator.config.cabin_count += 1;
                                let new_id = space_elevator.cabins.len();
                                space_elevator.cabins.push(Cabin::new(new_id));
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Mark complete
        {
            let mut projects = projects.write().await;
            if let Some(project) = projects.get_mut(&project_id) {
                project.status = ProjectStatus::Complete;
            }
        }

        // Send webhook
        let payload = ConstructionWebhookPayload::ConstructionComplete {
            project_id,
            project_type: project_snapshot.project_type.clone(),
            target_planet_id: project_snapshot.target_planet_id.clone(),
        };
        send_construction_webhook(
            &http_client,
            &callback_url,
            &payload,
            config.ship.webhook_timeout_secs,
            project_id,
        )
        .await;

        info!(project_id = %project_id, "Upgrade project completed");
    });
}

async fn apply_install_station(
    galaxy: &Arc<RwLock<crate::state::GalaxyState>>,
    config: &AppConfig,
    project: &ConstructionProject,
) {
    let mut galaxy = galaxy.write().await;
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == project.target_planet_id {
                if let PlanetStatus::Settled { settlement } = &planet.status {
                    let station_name = project
                        .station_name
                        .clone()
                        .unwrap_or_else(|| format!("Station-{}", &project.target_planet_id));
                    let elevator_config = SpaceElevatorConfig::default();
                    let cabins = (0..elevator_config.cabin_count)
                        .map(Cabin::new)
                        .collect();
                    let space_elevator = SpaceElevator {
                        warehouse: Warehouse {
                            owner_id: project.owner_id.clone(),
                            inventory: Default::default(),
                        },
                        config: elevator_config,
                        cabins,
                    };
                    let station = Station {
                        name: station_name,
                        owner_id: project.owner_id.clone(),
                        inventory: Default::default(),
                        mass_driver: Some(MassDriver::new(
                            config.mass_driver.default_channels,
                        )),
                        docking_bays: config.construction.initial_docking_bays,
                        max_storage: config.construction.initial_max_storage,
                    };
                    planet.status = PlanetStatus::Connected {
                        settlement: settlement.clone(),
                        station,
                        space_elevator,
                    };
                }
                return;
            }
        }
    }
}

async fn apply_found_settlement(
    galaxy: &Arc<RwLock<crate::state::GalaxyState>>,
    config: &AppConfig,
    project: &ConstructionProject,
) {
    let mut galaxy = galaxy.write().await;
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == project.target_planet_id {
                if matches!(planet.status, PlanetStatus::Uninhabited) {
                    let settlement_name = project
                        .settlement_name
                        .clone()
                        .unwrap_or_else(|| format!("Settlement-{}", &project.target_planet_id));
                    let station_name = project
                        .station_name
                        .clone()
                        .unwrap_or_else(|| format!("Station-{}", &project.target_planet_id));

                    let settlement = Settlement {
                        name: settlement_name,
                        economy: EconomyState::default(),
                        founding_goods: project.goods_consumed.clone(),
                    };

                    let elevator_config = SpaceElevatorConfig::default();
                    let cabins = (0..elevator_config.cabin_count)
                        .map(Cabin::new)
                        .collect();
                    let space_elevator = SpaceElevator {
                        warehouse: Warehouse {
                            owner_id: project.owner_id.clone(),
                            inventory: Default::default(),
                        },
                        config: elevator_config,
                        cabins,
                    };

                    // Extra goods go into the new station's inventory
                    let station = Station {
                        name: station_name,
                        owner_id: project.owner_id.clone(),
                        inventory: project.extra_goods.clone(),
                        mass_driver: Some(MassDriver::new(
                            config.mass_driver.default_channels,
                        )),
                        docking_bays: config.construction.initial_docking_bays,
                        max_storage: config.construction.initial_max_storage,
                    };

                    planet.status = PlanetStatus::Connected {
                        settlement,
                        station,
                        space_elevator,
                    };
                }
                return;
            }
        }
    }
}

async fn send_construction_webhook(
    http_client: &reqwest::Client,
    callback_url: &str,
    payload: &ConstructionWebhookPayload,
    timeout_secs: u64,
    project_id: Uuid,
) {
    if callback_url.is_empty() {
        return;
    }
    let timeout = Duration::from_secs(timeout_secs);
    match http_client
        .post(callback_url)
        .json(payload)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(resp) => {
            info!(project_id = %project_id, status = %resp.status(), "Construction webhook sent");
        }
        Err(e) => {
            warn!(project_id = %project_id, error = %e, "Failed to send construction webhook (non-fatal)");
        }
    }
}
