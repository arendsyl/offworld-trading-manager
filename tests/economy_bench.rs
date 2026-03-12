//! Economy tick benchmarks.
//!
//! Run with: cargo test --release -p offworld-trading-manager --test economy_bench -- --nocapture

use std::collections::HashMap;
use std::time::{Duration, Instant};

use offworld_trading_manager::economy::config::{
    FactoryTypeConfig, GlobalEconomyConfig, GoodConfig, PlanetEconomyConfig,
};
use offworld_trading_manager::economy::models::EconomyState;
use offworld_trading_manager::economy::tick::{economy_tick, initialize_economy_with_population};
use offworld_trading_manager::models::PlanetResource;

const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data");

// ---------------------------------------------------------------------------
// Helpers — load production data
// ---------------------------------------------------------------------------

fn load_production_global() -> GlobalEconomyConfig {
    let factories: Vec<FactoryTypeConfig> =
        serde_json::from_str(&std::fs::read_to_string(format!("{DATA_DIR}/factories.json")).unwrap())
            .unwrap();
    let consumption_profiles: HashMap<String, HashMap<String, f64>> =
        serde_json::from_str(
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
                resources.entry(output.good.clone()).or_insert(PlanetResource {
                    max_capacity: 10_000.0,
                    renewable: true,
                    regeneration_rate: 50.0,
                    max_extraction: 200.0,
                    k_half: 50.0,
                });
            }
        }
    }
    resources
}

fn init_settlement(
    global: &GlobalEconomyConfig,
    planet: &PlanetEconomyConfig,
    planet_resources: &HashMap<String, PlanetResource>,
    pop: f64,
) -> EconomyState {
    initialize_economy_with_population(global, planet, pop, planet_resources)
}

/// Warm up an economy by running a few ticks so it reaches a realistic state.
fn warm_up(
    econ: &mut EconomyState,
    global: &GlobalEconomyConfig,
    planet: &PlanetEconomyConfig,
    planet_resources: &HashMap<String, PlanetResource>,
    climate_key: &str,
    ticks: usize,
) {
    for _ in 0..ticks {
        economy_tick(econ, global, planet, planet_resources, climate_key);
    }
}

// ---------------------------------------------------------------------------
// Bench harness — manual since we don't want a criterion dependency
// ---------------------------------------------------------------------------

struct BenchResult {
    name: String,
    iterations: u64,
    total: Duration,
    min: Duration,
    max: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
}

impl BenchResult {
    fn mean(&self) -> Duration {
        self.total / self.iterations as u32
    }
}

impl std::fmt::Display for BenchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<45} {:>8} iters | mean {:>9.2?} | min {:>9.2?} | p50 {:>9.2?} | p95 {:>9.2?} | p99 {:>9.2?} | max {:>9.2?}",
            self.name, self.iterations, self.mean(), self.min, self.p50, self.p95, self.p99, self.max,
        )
    }
}

fn bench<F: FnMut()>(name: &str, target_duration: Duration, mut f: F) -> BenchResult {
    // Warmup: 10% of target or 100ms min
    let warmup_target = target_duration / 10;
    let warmup_start = Instant::now();
    while warmup_start.elapsed() < warmup_target {
        f();
    }

    // Collect samples
    let mut samples: Vec<Duration> = Vec::new();
    let bench_start = Instant::now();
    while bench_start.elapsed() < target_duration {
        let start = Instant::now();
        f();
        samples.push(start.elapsed());
    }

    samples.sort();
    let n = samples.len() as u64;
    let total: Duration = samples.iter().sum();

    BenchResult {
        name: name.to_string(),
        iterations: n,
        total,
        min: samples[0],
        max: *samples.last().unwrap(),
        p50: samples[(n as f64 * 0.50) as usize],
        p95: samples[(n as f64 * 0.95).min((n - 1) as f64) as usize],
        p99: samples[(n as f64 * 0.99).min((n - 1) as f64) as usize],
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_economy_tick() {
    let target = Duration::from_secs(3);
    let global = load_production_global();
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: 100_000.0,
        ..Default::default()
    };
    let planet_resources = make_planet_resources(&global);
    let climate = "temperate";

    let mut results: Vec<BenchResult> = Vec::new();

    // --- Single tick, small settlement (100 pop) ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 100.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);
        let snapshot = econ;

        results.push(bench("single_tick / 100 pop", target, || {
            let mut e = snapshot.clone();
            economy_tick(&mut e, &global, &planet, &planet_resources, climate);
        }));
    }

    // --- Single tick, medium settlement (10k pop) ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 10_000.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);
        let snapshot = econ.clone();

        results.push(bench("single_tick / 10k pop", target, || {
            let mut e = snapshot.clone();
            economy_tick(&mut e, &global, &planet, &planet_resources, climate);
        }));
    }

    // --- Single tick, large settlement (1M pop) ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 1_000_000.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);
        let snapshot = econ.clone();

        results.push(bench("single_tick / 1M pop", target, || {
            let mut e = snapshot.clone();
            economy_tick(&mut e, &global, &planet, &planet_resources, climate);
        }));
    }

    // --- Single tick with trade flows (imports + exports) ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 10_000.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);

        let mut snapshot = econ.clone();
        // Simulate active trade: 10 goods being imported/exported
        for factory in global.factory_types.iter().take(10) {
            for output in &factory.outputs {
                snapshot
                    .imports_this_tick
                    .insert(output.good.clone(), 100.0);
                snapshot
                    .exports_this_tick
                    .insert(output.good.clone(), 50.0);
            }
        }

        results.push(bench("single_tick / 10k pop + trades", target, || {
            let mut e = snapshot.clone();
            economy_tick(&mut e, &global, &planet, &planet_resources, climate);
        }));
    }

    // --- Galaxy tick: N settlements (simulates the snapshot-compute-writeback loop) ---
    for n_settlements in [2, 10, 50, 100] {
        let snapshots: Vec<EconomyState> = (0..n_settlements)
            .map(|_| {
                let mut e = init_settlement(&global, &planet, &planet_resources, 10_000.0);
                warm_up(&mut e, &global, &planet, &planet_resources, climate, 50);
                e
            })
            .collect();

        results.push(bench(
            &format!("galaxy_tick / {n_settlements} settlements"),
            target,
            || {
                let mut snaps: Vec<EconomyState> = snapshots.clone();
                for econ in &mut snaps {
                    economy_tick(econ, &global, &planet, &planet_resources, climate);
                }
            },
        ));
    }

    // --- Clone cost: measure EconomyState clone overhead ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 10_000.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);

        results.push(bench("clone_economy_state / 10k pop", target, || {
            let _ = std::hint::black_box(econ.clone());
        }));
    }

    // --- Sustained ticks: 1000 consecutive ticks (tests stability + cache behavior) ---
    {
        let mut econ = init_settlement(&global, &planet, &planet_resources, 10_000.0);
        warm_up(&mut econ, &global, &planet, &planet_resources, climate, 50);

        let start = Instant::now();
        let n_ticks = 1000;
        for _ in 0..n_ticks {
            economy_tick(&mut econ, &global, &planet, &planet_resources, climate);
        }
        let elapsed = start.elapsed();
        let per_tick = elapsed / n_ticks;

        // Not using bench() here since we want sequential state evolution
        println!(
            "{:<45} {:>8} iters | mean {:>9.2?} | total {:>9.2?}",
            "sustained_1000_ticks / 10k pop", n_ticks, per_tick, elapsed
        );

        // Verify the economy didn't blow up (soft checks — print warnings, don't fail)
        let pop = econ.demographics.total_population();
        assert!(pop.is_finite(), "population is NaN/Inf after 1000 ticks");
        assert!(pop < 1e12, "population exploded to {pop}");
        if pop < 1.0 {
            println!("  WARNING: population collapsed to {pop:.6} after 1000 ticks");
        }
        assert!(econ.wage > 0.0, "wage went to zero");
        assert!(econ.wage < 1e8, "wage exploded to {}", econ.wage);
        for (good, &price) in &econ.prices {
            assert!(price >= 0.01, "price of {good} collapsed to {price}");
            assert!(price <= 100_000.0, "price of {good} exploded to {price}");
        }
        println!(
            "  stability: pop={:.0}, wage={:.2}, unemployment={:.4}, income={:.0}",
            pop, econ.wage, econ.unemployment, econ.national_income
        );
    }

    // --- Print all results ---
    println!("\n{}", "=".repeat(130));
    println!("ECONOMY TICK BENCHMARKS (cargo test --release)");
    println!("{}", "=".repeat(130));
    for r in &results {
        println!("{r}");
    }
    println!("{}", "=".repeat(130));

    // --- Max frequency estimate ---
    let galaxy_2 = results.iter().find(|r| r.name.contains("2 settlements")).unwrap();
    let galaxy_10 = results.iter().find(|r| r.name.contains("10 settlements")).unwrap();
    let galaxy_100 = results.iter().find(|r| r.name.contains("100 settlements")).unwrap();

    println!("\nMAX TICK FREQUENCY ESTIMATES (compute only, no lock/sleep overhead):");
    println!(
        "  2 settlements:   {:.0} Hz  (p99 {:.2?} per tick)",
        1.0 / galaxy_2.p99.as_secs_f64(),
        galaxy_2.p99
    );
    println!(
        "  10 settlements:  {:.0} Hz  (p99 {:.2?} per tick)",
        1.0 / galaxy_10.p99.as_secs_f64(),
        galaxy_10.p99
    );
    println!(
        "  100 settlements: {:.0} Hz  (p99 {:.2?} per tick)",
        1.0 / galaxy_100.p99.as_secs_f64(),
        galaxy_100.p99
    );

    // Practical limit: leave 50% headroom for lock contention + trade lifecycle
    let practical_hz_2 = 0.5 / galaxy_2.p95.as_secs_f64();
    let practical_hz_100 = 0.5 / galaxy_100.p95.as_secs_f64();
    println!("\nPRACTICAL LIMIT (50% headroom for locks + trade + GC):");
    println!("  2 settlements:   {practical_hz_2:.0} Hz");
    println!("  100 settlements: {practical_hz_100:.0} Hz");
    println!("{}", "=".repeat(130));
}
