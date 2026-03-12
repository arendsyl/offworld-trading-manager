use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::debug;

use crate::config::AppConfig;
use crate::economy::config::{default_people_per_infra_unit, PlanetEconomyConfig};
use crate::economy::tick::{economy_tick, initialize_economy_with_population};
use crate::models::PlanetResource;
use crate::state::GalaxyState;

pub fn spawn_economy_loop(galaxy: Arc<RwLock<GalaxyState>>, config: Arc<AppConfig>) {
    tokio::spawn(async move {
        let tick = Duration::from_secs_f64(config.economy.tick_duration_secs);

        // Bootstrap: initialize seed settlements that haven't been initialized
        {
            let mut galaxy = galaxy.write().await;
            for system in galaxy.systems.values_mut() {
                for planet in &mut system.planets {
                    let planet_res = planet.resources.clone();
                    let planet_econ_config = planet.economy_config.clone();
                    let climate_key = planet.planet_type.to_string();
                    if let Some(settlement) = planet.settlement_mut() {
                        if !settlement.economy.initialized {
                            settlement.economy = initialize_economy_with_population(
                                &config.economy,
                                &planet_econ_config,
                                planet_econ_config.initial_pop_active,
                                &planet_res,
                            );
                        } else if settlement.economy.stocks.is_empty() {
                            // Backfill stocks for existing economies loaded without them
                            let pop = settlement.economy.demographics.total_population();
                            for factory in &config.economy.factory_types {
                                for output in &factory.outputs {
                                    if !config.economy.transient_goods.contains(&output.good) {
                                        settlement
                                            .economy
                                            .stocks
                                            .entry(output.good.clone())
                                            .or_insert(pop * 0.5);
                                    }
                                }
                            }
                        }
                        // Backfill infrastructure for existing economies
                        if settlement.economy.initialized
                            && settlement.economy.infrastructure == 0.0
                        {
                            let pop =
                                settlement.economy.demographics.total_population();
                            let base = planet_econ_config.base_carrying_capacity;
                            let ppu = config
                                .economy
                                .infra_climate
                                .get(&climate_key)
                                .map(|c| c.people_per_unit)
                                .unwrap_or(default_people_per_infra_unit());
                            if ppu > 0.0 && pop > base {
                                settlement.economy.infrastructure =
                                    (pop - base) / ppu;
                            }
                        }
                    }
                }
            }
        }

        loop {
            tokio::time::sleep(tick).await;

            // Read: snapshot economies + drain trade accumulators
            let mut snapshots: Vec<(
                String,
                String,
                crate::economy::models::EconomyState,
                HashMap<String, PlanetResource>,
                PlanetEconomyConfig,
                String, // climate_key
            )> = Vec::new();
            {
                let mut galaxy = galaxy.write().await;
                for (sys_name, system) in galaxy.systems.iter_mut() {
                    for planet in &mut system.planets {
                        let planet_res = planet.resources.clone();
                        let planet_econ_config = planet.economy_config.clone();
                        let ck = planet.planet_type.to_string();
                        if let Some(settlement) = planet.settlement_mut() {
                            let mut econ = settlement.economy.clone();
                            econ.imports_this_tick =
                                std::mem::take(&mut settlement.economy.imports_this_tick);
                            econ.exports_this_tick =
                                std::mem::take(&mut settlement.economy.exports_this_tick);
                            snapshots.push((
                                sys_name.clone(),
                                planet.id.clone(),
                                econ,
                                planet_res,
                                planet_econ_config,
                                ck,
                            ));
                        }
                    }
                }
            }

            for (_, _, econ, planet_resources, planet_econ_config, climate_key) in &mut snapshots {
                economy_tick(
                    econ,
                    &config.economy,
                    planet_econ_config,
                    planet_resources,
                    climate_key,
                );
            }

            // Write back results
            {
                let mut galaxy = galaxy.write().await;
                for (sys_name, planet_id, econ, _, _, _) in snapshots {
                    if let Some(system) = galaxy.systems.get_mut(&sys_name) {
                        if let Some(planet) =
                            system.planets.iter_mut().find(|p| p.id == planet_id)
                        {
                            if let Some(settlement) = planet.settlement_mut() {
                                let mut final_econ = econ;
                                for (k, v) in &settlement.economy.imports_this_tick {
                                    *final_econ
                                        .imports_this_tick
                                        .entry(k.clone())
                                        .or_default() += v;
                                }
                                for (k, v) in &settlement.economy.exports_this_tick {
                                    *final_econ
                                        .exports_this_tick
                                        .entry(k.clone())
                                        .or_default() += v;
                                }
                                settlement.economy = final_econ;
                            }
                        }
                    }
                }
            }

            debug!("Economy tick completed");
        }
    });
}
