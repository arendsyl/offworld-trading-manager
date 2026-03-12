//! Reusable economy simulation runner.
//! Runs N ticks, collects per-tick snapshots, and prints a structured report:
//!   1. Production summary (per-good totals)
//!   2. Final state overview
//!   3. 10-tick demographic/macro breakdown
//!
//! Usage:
//!   use economy_sim_helper::{SimConfig, run_simulation};
//!   let cfg = SimConfig { ticks: 200, pop: 10_000.0, ..Default::default() };
//!   run_simulation(cfg);

use std::collections::HashMap;

use offworld_trading_manager::economy::config::{
    FactoryTypeConfig, GlobalEconomyConfig, GoodConfig, PlanetEconomyConfig,
};
use offworld_trading_manager::economy::models::EconomyState;
use offworld_trading_manager::economy::tick::{economy_tick, initialize_economy_with_population};
use offworld_trading_manager::models::PlanetResource;

const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data");

// ── Configuration ────────────────────────────────────────────────────

pub struct SimConfig {
    pub ticks: usize,
    pub pop: f64,
    pub climate: String,
    pub base_carrying_capacity: f64,
    /// Print the 10-tick macro table (demographics, wage, etc.)
    pub show_macro_table: bool,
    /// Print per-good production totals
    pub show_production: bool,
    /// Print top prices at end
    pub show_prices: bool,
    /// Print labor allocation at end
    pub show_labor: bool,
    /// Print starved consumption goods at end
    pub show_starved: bool,
    /// Print capital at end
    pub show_capital: bool,
    /// Macro table shows every Nth tick (e.g. 10 = one row per 10 ticks)
    pub macro_tick_modulo: usize,
    /// Show every tick for the first N ticks before switching to modulo
    pub macro_initial_ticks: usize,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            ticks: 200,
            pop: 10_000.0,
            climate: "temperate".into(),
            base_carrying_capacity: 100_000.0,
            show_macro_table: true,
            show_production: true,
            show_prices: true,
            show_labor: true,
            show_starved: true,
            show_capital: false,
            macro_tick_modulo: 10,
            macro_initial_ticks: 5,
        }
    }
}

// ── Per-tick snapshot ─────────────────────────────────────────────────

struct TickSnapshot {
    tick: usize,
    pop: f64,
    active: f64,
    young: f64,
    old: f64,
    wage: f64,
    unemployment: f64,
    income: f64,
    savings: f64,
    fulfillment: f64,
    crowding: f64,
    infrastructure: f64,
}

// ── Public entry point ───────────────────────────────────────────────

pub struct SimResult {
    pub econ: EconomyState,
    pub global: GlobalEconomyConfig,
}

pub fn run_simulation(cfg: SimConfig) -> SimResult {
    let global = load_global();
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: cfg.base_carrying_capacity,
        ..Default::default()
    };
    let planet_resources = make_planet_resources(&global);

    let mut econ = initialize_economy_with_population(&global, &planet, cfg.pop, &planet_resources);

    let mut snapshots: Vec<TickSnapshot> = Vec::new();
    let mut cumulative_production: HashMap<String, f64> = HashMap::new();

    let consumption_goods: Vec<String> = global
        .consumption_profiles
        .get(&cfg.climate)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();

    for tick in 0..cfg.ticks {
        // Compute consumption fulfillment before tick mutates state
        let fulfillment = compute_fulfillment(&econ, &consumption_goods);

        snapshots.push(TickSnapshot {
            tick,
            pop: econ.demographics.total_population(),
            active: econ.demographics.pop_active,
            young: econ.demographics.pop_young,
            old: econ.demographics.pop_old,
            wage: econ.wage,
            unemployment: econ.unemployment,
            income: econ.national_income,
            savings: econ.savings_rate,
            fulfillment,
            crowding: econ.last_crowding,
            infrastructure: econ.infrastructure,
        });

        economy_tick(&mut econ, &global, &planet, &planet_resources, &cfg.climate);

        // Accumulate production
        for (good, &qty) in &econ.last_production {
            *cumulative_production.entry(good.clone()).or_default() += qty;
        }
    }

    // ── Report ───────────────────────────────────────────────────────

    let sep = "=".repeat(80);
    println!("\n{}", sep);
    println!(
        "  ECONOMY SIMULATION: {} ticks, pop={}, climate={}",
        cfg.ticks, cfg.pop, cfg.climate
    );
    println!("{}\n", sep);

    // 1. Production summary
    if cfg.show_production {
        print_production(&cumulative_production, &econ, cfg.ticks);
    }

    // 2. Starved goods
    if cfg.show_starved {
        print_starved(&econ, &consumption_goods);
    }

    // 3. Labor allocation
    if cfg.show_labor {
        print_labor(&econ, &global);
    }

    // 4. Prices
    if cfg.show_prices {
        print_prices(&econ);
    }

    // 5. Capital
    if cfg.show_capital {
        print_capital(&econ);
    }

    // 6. Macro table (10-tick breakdown) — at the end
    if cfg.show_macro_table {
        print_macro_table(&snapshots, cfg.macro_tick_modulo, cfg.macro_initial_ticks);
    }

    SimResult { econ, global }
}

// ── Data loading ─────────────────────────────────────────────────────

fn load_global() -> GlobalEconomyConfig {
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

// ── Helpers ──────────────────────────────────────────────────────────

fn compute_fulfillment(econ: &EconomyState, consumption_goods: &[String]) -> f64 {
    if consumption_goods.is_empty() {
        return 1.0;
    }
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
}

fn print_production(cumulative: &HashMap<String, f64>, econ: &EconomyState, ticks: usize) {
    println!("── PRODUCTION (cumulative over {} ticks) ──", ticks);
    let mut goods: Vec<(&String, &f64)> = cumulative.iter().collect();
    goods.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

    println!(
        "  {:<30} {:>12} {:>12} {:>10}",
        "Good", "Total", "Per-tick", "Stock"
    );
    for (good, total) in &goods {
        let per_tick = *total / ticks as f64;
        let stock = econ.stocks.get(*good).copied().unwrap_or(0.0);
        println!(
            "  {:<30} {:>12.1} {:>12.2} {:>10.1}",
            good, total, per_tick, stock
        );
    }
    println!();
}

fn print_starved(econ: &EconomyState, consumption_goods: &[String]) {
    let mut starved: Vec<(String, f64, f64, f64, f64)> = Vec::new();
    for g in consumption_goods {
        let desired = econ.last_demand_c.get(g).copied().unwrap_or(0.0);
        let actual = econ.last_consumption.get(g).copied().unwrap_or(0.0);
        let stock = econ.stocks.get(g).copied().unwrap_or(0.0);
        let produced = econ.last_production.get(g).copied().unwrap_or(0.0);
        if desired > 0.0 && actual / desired < 0.9 {
            starved.push((g.clone(), desired, actual, stock, produced));
        }
    }
    if starved.is_empty() {
        println!("── CONSUMPTION: all goods fulfilled (>90%) ──\n");
    } else {
        println!("── STARVED CONSUMPTION GOODS (<90% fulfilled) ──");
        println!(
            "  {:<30} {:>10} {:>10} {:>10} {:>10} {:>8}",
            "Good", "Desired", "Actual", "Stock", "Produced", "Ratio"
        );
        for (g, desired, actual, stock, produced) in &starved {
            println!(
                "  {:<30} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>8.4}",
                g,
                desired,
                actual,
                stock,
                produced,
                if *desired > 0.0 {
                    actual / desired
                } else {
                    1.0
                }
            );
        }
        println!();
    }
}

fn print_labor(econ: &EconomyState, global: &GlobalEconomyConfig) {
    let mut labor: Vec<(String, f64, f64)> = econ
        .labor_alloc
        .iter()
        .map(|(id, &l)| {
            let va = global
                .factory_types
                .iter()
                .find(|f| f.id == *id)
                .map(|factory| {
                    let output_val: f64 = factory
                        .outputs
                        .iter()
                        .map(|gq| {
                            gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                        })
                        .sum();
                    let input_val: f64 = factory
                        .inputs
                        .iter()
                        .map(|gq| {
                            gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                        })
                        .sum();
                    output_val - input_val
                })
                .unwrap_or(0.0);
            (id.clone(), l, va)
        })
        .collect();
    labor.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let total_labor: f64 = labor.iter().map(|(_, l, _)| l).sum();
    println!("── LABOR ALLOCATION (top 15, total={:.0}) ──", total_labor);
    println!("  {:<35} {:>10} {:>10}", "Factory", "Labor", "VA");
    for (id, l, va) in labor.iter().take(15) {
        println!("  {:<35} {:>10.1} {:>10.2}", id, l, va);
    }
    println!();
}

fn print_prices(econ: &EconomyState) {
    let mut prices: Vec<(String, f64)> = econ.prices.iter().map(|(k, &v)| (k.clone(), v)).collect();
    prices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("── PRICES (top 10 / bottom 5) ──");
    for (g, p) in prices.iter().take(10) {
        println!("  {:<35} {:>10.4}", g, p);
    }
    println!("  ...");
    for (g, p) in prices.iter().rev().take(5) {
        println!("  {:<35} {:>10.4}", g, p);
    }
    println!();
}

fn print_capital(econ: &EconomyState) {
    let mut capital: Vec<(String, f64)> =
        econ.capital.iter().map(|(k, &v)| (k.clone(), v)).collect();
    capital.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("── CAPITAL (top 15) ──");
    println!("  {:<35} {:>10}", "Factory", "Capital");
    for (id, k) in capital.iter().take(15) {
        println!("  {:<35} {:>10.2}", id, k);
    }
    println!();
}

fn print_macro_table(snapshots: &[TickSnapshot], modulo: usize, initial_ticks: usize) {
    let modulo = modulo.max(1);
    println!("── MACRO ({}-tick breakdown) ──", modulo);
    println!(
        "  {:<6} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8} {:>10} {:>8} {:>8} {:>8}",
        "tick",
        "pop",
        "active",
        "young",
        "old",
        "wage",
        "unempl",
        "income",
        "save",
        "fulfill",
        "crowd"
    );

    for snap in snapshots {
        if snap.tick % modulo == 0 || snap.tick < initial_ticks {
            println!(
                "  {:<6} {:>10.1} {:>10.1} {:>10.1} {:>10.1} {:>10.2} {:>8.4} {:>10.0} {:>8.4} {:>8.4} {:>8.4}",
                snap.tick,
                snap.pop,
                snap.active,
                snap.young,
                snap.old,
                snap.wage,
                snap.unemployment,
                snap.income,
                snap.savings,
                snap.fulfillment,
                snap.crowding,
            );
        }
    }
    println!();
}

// ── Test entry point ─────────────────────────────────────────────────

#[test]
fn sim_default() {
    run_simulation(SimConfig::default());
}
