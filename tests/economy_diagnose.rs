//! Diagnose population collapse in sustained economy ticks.
//! Run with: cargo test --test economy_diagnose -- --nocapture

use std::collections::HashMap;

use offworld_trading_manager::economy::config::{
    FactoryTypeConfig, GlobalEconomyConfig, GoodConfig, PlanetEconomyConfig,
};
use offworld_trading_manager::economy::tick::{economy_tick, initialize_economy_with_population};
use offworld_trading_manager::models::PlanetResource;

const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data");

fn load_production_global() -> GlobalEconomyConfig {
    let factories: Vec<FactoryTypeConfig> = serde_json::from_str(
        &std::fs::read_to_string(format!("{DATA_DIR}/factories.json")).unwrap(),
    )
    .unwrap();
    let consumption_profiles: HashMap<String, HashMap<String, f64>> = serde_json::from_str(
        &std::fs::read_to_string(format!("{DATA_DIR}/consumptions.json")).unwrap(),
    )
    .unwrap();
    let goods: Vec<GoodConfig> =
        serde_json::from_str(&std::fs::read_to_string(format!("{DATA_DIR}/goods.json")).unwrap())
            .unwrap();
    let transient_goods = goods
        .iter()
        .filter(|g| g.transient)
        .map(|g| g.id.clone())
        .collect();

    GlobalEconomyConfig {
        factory_types: factories,
        consumption_profiles,
        goods,
        transient_goods,
        ..Default::default()
    }
}

fn make_planet_resources(global: &GlobalEconomyConfig) -> HashMap<String, PlanetResource> {
    let mut resources = HashMap::new();
    for factory in &global.factory_types {
        if factory.is_extraction() {
            for output in &factory.outputs {
                resources
                    .entry(output.good.clone())
                    .or_insert(PlanetResource {
                        max_capacity: 1_000_000.0,
                        renewable: true,
                        regeneration_rate: 100_000.0,
                        max_extraction: 100_000.0,
                        k_half: 1.0,
                    });
            }
        }
    }
    resources
}

#[test]
fn diagnose_collapse() {
    let global = load_production_global();
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: 100_000.0,
        ..Default::default()
    };
    let planet_resources = make_planet_resources(&global);
    let climate = "temperate";

    let mut econ =
        initialize_economy_with_population(&global, &planet, 10_000.0, &planet_resources);

    println!(
        "{:<6} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "tick", "pop", "active", "young", "old", "wage", "unempl", "income", "savings", "fulfill"
    );

    for tick in 0..200 {
        // Compute consumption fulfillment before the tick mutates state
        let consumption_goods: Vec<String> = global
            .consumption_profiles
            .get(climate)
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default();

        let fulfillment = if !consumption_goods.is_empty() {
            let ratios: Vec<f64> = consumption_goods
                .iter()
                .map(|g| {
                    let desired = econ.last_demand_c.get(g).copied().unwrap_or(0.0);
                    let actual = econ.last_consumption.get(g).copied().unwrap_or(0.0);
                    if desired > 0.0 {
                        (actual / desired).min(1.0)
                    } else {
                        1.0
                    }
                })
                .collect();
            ratios.iter().sum::<f64>() / ratios.len() as f64
        } else {
            1.0
        };

        if tick % 10 == 0 || tick < 20 {
            println!(
                "{:<6} {:>10.1} {:>10.1} {:>10.1} {:>10.1} {:>10.2} {:>10.4} {:>10.0} {:>10.4} {:>10.4}",
                tick,
                econ.demographics.total_population(),
                econ.demographics.pop_active,
                econ.demographics.pop_young,
                econ.demographics.pop_old,
                econ.wage,
                econ.unemployment,
                econ.national_income,
                econ.savings_rate,
                fulfillment,
            );

            // Show which consumption goods are starved
            if tick % 50 == 0 {
                let mut starved: Vec<(String, f64, f64, f64)> = Vec::new();
                for g in &consumption_goods {
                    let desired = econ.last_demand_c.get(g).copied().unwrap_or(0.0);
                    let actual = econ.last_consumption.get(g).copied().unwrap_or(0.0);
                    let stock = econ.stocks.get(g).copied().unwrap_or(0.0);
                    if desired > 0.0 && actual / desired < 0.9 {
                        starved.push((g.clone(), desired, actual, stock));
                    }
                }
                if !starved.is_empty() {
                    println!("  STARVED GOODS:");
                    for (g, desired, actual, stock) in &starved {
                        let produced = econ.last_production.get(g.as_str()).copied().unwrap_or(0.0);
                        let d_g = econ.last_demand.get(g.as_str()).copied().unwrap_or(0.0);
                        let s_g = econ.last_available_supply.get(g.as_str()).copied().unwrap_or(0.0);
                        let signal = econ.last_price_signal.get(g.as_str()).copied().unwrap_or(0.0);
                        let price = econ.prices.get(g.as_str()).copied().unwrap_or(0.0);
                        println!(
                            "    {:<25} ratio={:.4}  D={:<10.1} S={:<10.1} signal={:+.4}  price={:.2}",
                            g,
                            if *desired > 0.0 { actual / desired } else { 1.0 },
                            d_g, s_g, signal, price,
                        );
                    }
                }

                // Show labor allocation for top factories
                let mut labor: Vec<(String, f64)> = econ
                    .labor_alloc
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                labor.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                println!(
                    "  TOP LABOR ALLOC (of {} total):",
                    labor.iter().map(|(_, v)| v).sum::<f64>() as u64
                );
                for (id, l) in labor.iter().take(10) {
                    let va = {
                        let factory = global.factory_types.iter().find(|f| f.id == *id).unwrap();
                        let output_val: f64 = factory
                            .outputs
                            .iter()
                            .map(|gq| {
                                gq.quantity as f64
                                    * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                            })
                            .sum();
                        let input_val: f64 = factory
                            .inputs
                            .iter()
                            .map(|gq| {
                                gq.quantity as f64
                                    * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                            })
                            .sum();
                        output_val - input_val
                    };
                    println!("    {:<30} L={:<10.1} VA={:<10.2}", id, l, va);
                }

                // Show prices of key goods
                println!("  KEY PRICES (with signal):");
                let mut prices: Vec<(String, f64)> =
                    econ.prices.iter().map(|(k, v)| (k.clone(), *v)).collect();
                prices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                for (g, p) in prices.iter().take(10) {
                    let sig = econ.last_price_signal.get(g.as_str()).copied().unwrap_or(0.0);
                    let d_g = econ.last_demand.get(g.as_str()).copied().unwrap_or(0.0);
                    let s_g = econ.last_available_supply.get(g.as_str()).copied().unwrap_or(0.0);
                    println!("    {:<30} {:.2}  sig={:+.4}  D={:.0} S={:.0}", g, p, sig, d_g, s_g);
                }
                for (g, p) in prices.iter().rev().take(5) {
                    let sig = econ.last_price_signal.get(g.as_str()).copied().unwrap_or(0.0);
                    println!("    {:<30} {:.2}  sig={:+.4}", g, p, sig);
                }
            }
        }

        economy_tick(&mut econ, &global, &planet, &planet_resources, climate);
    }
}
