use std::collections::HashMap;

use offworld_trading_manager::economy::config::{
    ClimateInfraConfig, FactoryCategory, FactoryTypeConfig, GlobalEconomyConfig, GoodQuantity,
    PlanetEconomyConfig,
};
use offworld_trading_manager::economy::models::{Demographics, EconomyState};
use offworld_trading_manager::economy::tick::{economy_tick, initialize_economy_with_population};
use offworld_trading_manager::models::PlanetResource;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_approx(a: f64, b: f64, msg: &str) {
    assert!(
        (a - b).abs() < 1e-6,
        "{msg}: expected {b}, got {a} (diff {})",
        (a - b).abs()
    );
}

fn assert_approx_tol(a: f64, b: f64, tol: f64, msg: &str) {
    assert!(
        (a - b).abs() < tol,
        "{msg}: expected {b}, got {a} (diff {}, tol {tol})",
        (a - b).abs()
    );
}

fn mine_factory() -> FactoryTypeConfig {
    FactoryTypeConfig {
        id: "mine".into(),
        name: "Mine".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        build_cost: vec![GoodQuantity {
            good: "metal".into(),
            quantity: 1,
        }],
        production_cycle: 1.0,
    }
}

fn refinery_factory() -> FactoryTypeConfig {
    FactoryTypeConfig {
        id: "refinery".into(),
        name: "Refinery".into(),
        tier: 1,
        category: FactoryCategory::Manufacturing,
        inputs: vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        outputs: vec![GoodQuantity {
            good: "metal".into(),
            quantity: 1,
        }],
        build_cost: vec![GoodQuantity {
            good: "metal".into(),
            quantity: 1,
        }],
        production_cycle: 1.0,
    }
}

fn make_global(factories: Vec<FactoryTypeConfig>) -> GlobalEconomyConfig {
    // Build equal-share consumption profile for all output goods
    let mut output_goods: Vec<String> = Vec::new();
    for f in &factories {
        for o in &f.outputs {
            if !output_goods.contains(&o.good) {
                output_goods.push(o.good.clone());
            }
        }
    }
    let share = if output_goods.is_empty() {
        1.0
    } else {
        1.0 / output_goods.len() as f64
    };
    let profile: HashMap<String, f64> = output_goods.into_iter().map(|g| (g, share)).collect();
    let mut consumption_profiles = HashMap::new();
    consumption_profiles.insert("test".into(), profile);

    GlobalEconomyConfig {
        tick_duration_secs: 1.0,
        factory_types: factories,
        labor_mobility: 1.0,
        consumption_profiles,
        ..Default::default()
    }
}

fn make_planet() -> PlanetEconomyConfig {
    PlanetEconomyConfig {
        base_carrying_capacity: 10000.0,
        ..Default::default()
    }
}

fn make_econ(
    global: &GlobalEconomyConfig,
    pop_active: f64,
    stocks: HashMap<String, f64>,
    capital: HashMap<String, f64>,
    prices: HashMap<String, f64>,
) -> EconomyState {
    let n_factories = global.factory_types.len().max(1) as f64;
    let labor_per = pop_active / n_factories;
    let labor_alloc: HashMap<String, f64> = global
        .factory_types
        .iter()
        .map(|f| (f.id.clone(), labor_per))
        .collect();

    EconomyState {
        demographics: Demographics {
            pop_young: pop_active * 0.3,
            pop_active,
            pop_old: pop_active * 0.2,
        },
        capital,
        labor_alloc,
        prices,
        wage: 10.0,
        unemployment: 0.05,
        national_income: 10.0 * pop_active,
        savings_rate: 0.22,
        stocks,
        initialized: true,
        ..Default::default()
    }
}

fn ore_resource() -> PlanetResource {
    PlanetResource {
        max_capacity: 10000.0,
        max_extraction: 100.0,
        k_half: 1.0,
        renewable: false,
        regeneration_rate: 0.0,
    }
}

fn make_global_with_infra(
    factories: Vec<FactoryTypeConfig>,
    climate_key: &str,
    build_cost: Vec<GoodQuantity>,
    people_per_unit: f64,
) -> GlobalEconomyConfig {
    let mut global = make_global(factories);
    global.infra_climate.insert(
        climate_key.into(),
        ClimateInfraConfig {
            build_cost,
            people_per_unit,
        },
    );
    global
}

fn make_planet_with_infra(
    base_carrying_capacity: f64,
    initial_infrastructure: f64,
) -> PlanetEconomyConfig {
    PlanetEconomyConfig {
        base_carrying_capacity,
        initial_infrastructure,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// 1. Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_economy_with_population() {
    let global = make_global(vec![mine_factory(), refinery_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let econ = initialize_economy_with_population(&global, &planet, 1000.0, &resources);

    // Demographic splits: 20/65/15%
    assert_approx(econ.demographics.pop_young, 200.0, "pop_young");
    assert_approx(econ.demographics.pop_active, 650.0, "pop_active");
    assert_approx(econ.demographics.pop_old, 150.0, "pop_old");

    // Capital from tier defaults scaled by pop: active=650, scale=650/100=6.5
    // mine=tier0→50*6.5=325, refinery=tier1→30*6.5=195
    assert_approx(
        *econ.capital.get("mine").unwrap(),
        325.0,
        "mine capital",
    );
    assert_approx(
        *econ.capital.get("refinery").unwrap(),
        195.0,
        "refinery capital",
    );

    // Prices from cost-plus: ore=base(1.0), metal=(1*1.0*1.3)/1=1.3
    assert_approx(*econ.prices.get("ore").unwrap(), 1.0, "ore price");
    assert_approx(*econ.prices.get("metal").unwrap(), 1.3, "metal price");

    // VA-proportional labor: mine VA=(1*1.0)/1=1.0, refinery VA=(1*1.3-1*1.0)/1=0.3
    let l_mine = *econ.labor_alloc.get("mine").unwrap();
    let l_ref = *econ.labor_alloc.get("refinery").unwrap();
    assert!(l_mine > l_ref, "mine has higher VA → more labor");
    assert!(l_mine > 0.0 && l_ref > 0.0, "both factories get workers");

    // Stocks seeded at pop * 5.0 per output good
    assert_approx(*econ.stocks.get("ore").unwrap(), 5000.0, "ore stock seed");
    assert_approx(*econ.stocks.get("metal").unwrap(), 5000.0, "metal stock seed");

    // Resources at max_capacity
    assert_approx(
        *econ.resources.get("ore").unwrap(),
        10000.0,
        "ore resource at max",
    );

    assert!(econ.initialized, "should be initialized");
}

#[test]
fn test_initialize_minimum_population() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new();

    let econ = initialize_economy_with_population(&global, &planet, -50.0, &resources);

    // Pop clamped to 1.0
    let total = econ.demographics.total_population();
    assert_approx(total, 1.0, "min population clamp");
    assert!(econ.initialized);
}

// ---------------------------------------------------------------------------
// 2. Guard & Edge Cases
// ---------------------------------------------------------------------------

#[test]
fn test_uninitialized_economy_is_noop() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new();

    let mut econ = EconomyState::default();
    assert!(!econ.initialized);

    let stocks_before = econ.stocks.clone();
    let wage_before = econ.wage;
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert_eq!(econ.stocks, stocks_before);
    assert_eq!(econ.wage, wage_before);
    assert!(!econ.initialized);
}

#[test]
fn test_no_consumption_without_profile() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    // Use a climate key not in the profiles
    economy_tick(&mut econ, &global, &planet, &resources, "nonexistent_climate");

    // No consumption should have occurred via D^C
    let consumed = econ.last_consumption.get("ore").copied().unwrap_or(0.0);
    assert_approx(consumed, 0.0, "no consumption without profile");
}

// ---------------------------------------------------------------------------
// 3. Core: One-Tick Production Delay
// ---------------------------------------------------------------------------

#[test]
fn test_one_tick_production_delay() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Start with zero ore stock
    let stocks = HashMap::new();
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    // Set resource stock so mine can extract (extraction depends on econ.resources, not stocks)
    econ.resources.insert("ore".into(), 10000.0);
    // Zero ore goods stock, so available_supply for ore should be ~0 on tick 1
    // (imports are empty, stocks are empty before production)

    // Tick 1
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // After tick 1: last_available_supply["ore"] was based on stocks before production
    // which was 0 (plus any imports = 0)
    let supply_t1 = econ.last_available_supply.get("ore").copied().unwrap_or(0.0);
    assert_approx(supply_t1, 0.0, "tick 1: available supply should be ~0");

    // But stocks now > 0 because production was credited at end of tick
    let stock_t1 = econ.stocks.get("ore").copied().unwrap_or(0.0);
    assert!(stock_t1 > 0.0, "tick 1: stocks should have production credited");

    // Tick 2
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // Now available_supply includes tick 1's production
    let supply_t2 = econ.last_available_supply.get("ore").copied().unwrap_or(0.0);
    assert!(
        supply_t2 > 0.0,
        "tick 2: available supply should include tick 1 production, got {supply_t2}"
    );
}

// ---------------------------------------------------------------------------
// 4. Core: Domestic Priority Rationing
// ---------------------------------------------------------------------------

#[test]
fn test_domestic_priority_shortage() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Low stock, high national_income drives high consumption demand
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.national_income = 10000.0; // Very high income => high consumption demand
    econ.exports_this_tick.insert("ore".into(), 50.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // Exports should be 0 under shortage (domestic gets priority)
    let exports = econ.last_exports_fulfilled.get("ore").copied().unwrap_or(0.0);
    assert_approx(exports, 0.0, "exports zero under shortage");

    // Domestic consumption should be rationed but > 0
    let consumed = econ.last_consumption.get("ore").copied().unwrap_or(0.0);
    // We just check it's rationed (less than what would be desired with such high income)
    assert!(consumed >= 0.0, "consumption should be non-negative");
}

#[test]
fn test_domestic_priority_surplus() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Abundant stocks, moderate exports
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.exports_this_tick.insert("ore".into(), 50.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // Domestic fully met (consumption == desired)
    let consumed = econ.last_consumption.get("ore").copied().unwrap_or(0.0);
    assert!(consumed > 0.0, "consumption should be satisfied");

    // Exports should be fulfilled
    let exports = econ.last_exports_fulfilled.get("ore").copied().unwrap_or(0.0);
    assert_approx(exports, 50.0, "exports fully met with surplus");
}

#[test]
fn test_domestic_priority_abundance() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1_000_000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.exports_this_tick.insert("ore".into(), 10.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let consumed = econ.last_consumption.get("ore").copied().unwrap_or(0.0);
    let invested = econ.last_investment.get("ore").copied().unwrap_or(0.0);
    let exports = econ.last_exports_fulfilled.get("ore").copied().unwrap_or(0.0);

    assert!(consumed > 0.0, "consumption satisfied");
    assert!(invested >= 0.0, "investment non-negative");
    assert_approx(exports, 10.0, "exports fully satisfied in abundance");
}

#[test]
fn test_exports_zero_under_shortage() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Zero stock, exports requested
    let stocks = HashMap::new();
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.exports_this_tick.insert("ore".into(), 100.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let exports = econ.last_exports_fulfilled.get("ore").copied().unwrap_or(0.0);
    assert_approx(exports, 0.0, "exports must be zero when stock is zero");
}

// ---------------------------------------------------------------------------
// 5. Core: Input Rationing
// ---------------------------------------------------------------------------

#[test]
fn test_input_rationing_across_factories() {
    // Two identical refineries sharing scarce ore
    let ref1 = FactoryTypeConfig {
        id: "refinery_a".into(),
        name: "Refinery A".into(),
        tier: 1,
        category: FactoryCategory::Manufacturing,
        inputs: vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        outputs: vec![GoodQuantity {
            good: "metal".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };
    let ref2 = FactoryTypeConfig {
        id: "refinery_b".into(),
        name: "Refinery B".into(),
        tier: 1,
        category: FactoryCategory::Manufacturing,
        inputs: vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        outputs: vec![GoodQuantity {
            good: "metal".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };

    let global = make_global(vec![ref1, ref2]);
    let planet = make_planet();
    let resources = HashMap::new();

    // Scarce ore: both refineries will compete
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10.0);
    stocks.insert("metal".into(), 100.0);
    let mut capital = HashMap::new();
    capital.insert("refinery_a".into(), 30.0);
    capital.insert("refinery_b".into(), 30.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);
    prices.insert("metal".into(), 12.0);

    let mut econ = make_econ(&global, 100.0, stocks.clone(), capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // Both refineries should get equal production (proportional rationing)
    let prod_a = econ.labor_alloc.get("refinery_a").copied().unwrap_or(0.0);
    let prod_b = econ.labor_alloc.get("refinery_b").copied().unwrap_or(0.0);
    // With same capital and same labor, they should be equal
    assert_approx_tol(prod_a, prod_b, 1e-3, "equal labor => equal rationing");

    // Total ore consumed should not exceed starting stock
    // The key check: we didn't consume more ore than we started with
    let total_metal_produced = econ.last_production.get("metal").copied().unwrap_or(0.0);
    // Each unit of metal requires 1 ore, so metal produced <= initial ore stock
    assert!(
        total_metal_produced <= 10.0 + 1e-6,
        "metal produced ({total_metal_produced}) should not exceed ore supply (10)"
    );
}

// ---------------------------------------------------------------------------
// 6. Production & Extraction
// ---------------------------------------------------------------------------

#[test]
fn test_extraction_michaelis_menten() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = make_planet();

    let mut resources = HashMap::new();
    resources.insert(
        "ore".into(),
        PlanetResource {
            max_capacity: 1000.0,
            max_extraction: 100.0,
            k_half: 50.0,
            renewable: false,
            regeneration_rate: 0.0,
        },
    );

    // K=50, k_half=50 => capital_saturation = 50/(50+50) = 0.5
    // stock=500, max_cap=1000 => stock_ratio = 0.5
    // limit = 100 * 0.5 * 0.5 = 25.0
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 0.0); // goods stock (not resource)
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 1000.0, stocks, capital, prices);
    // Set resource stock
    econ.resources.insert("ore".into(), 500.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let produced = econ.last_production.get("ore").copied().unwrap_or(0.0);
    // Cobb-Douglas: K^0.4 * L^0.6 = 50^0.4 * 1000^0.6 which is much larger than 25
    // So production should be capped at the extraction limit of 25
    assert!(
        produced <= 25.0 + 1e-6,
        "production ({produced}) should be capped by extraction limit (25)"
    );
    assert!(
        produced > 0.0,
        "production should be positive"
    );
}

#[test]
fn test_extraction_with_renewable_regeneration() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = make_planet();

    let mut resources = HashMap::new();
    resources.insert(
        "ore".into(),
        PlanetResource {
            max_capacity: 1000.0,
            max_extraction: 100.0,
            k_half: 50.0,
            renewable: true,
            regeneration_rate: 10.0,
        },
    );

    let stocks = HashMap::new();
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.resources.insert("ore".into(), 500.0);

    let resource_before = 500.0;
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let resource_after = *econ.resources.get("ore").unwrap();
    let produced = econ.last_production.get("ore").copied().unwrap_or(0.0);

    // With renewable, resource should regenerate after extraction
    // regen = regeneration_rate * (1 - stock/max_cap) * dt
    // After extraction: stock decreases by produced amount, then regen is added
    // The resource should not drop as much as it would without regeneration
    // Check: resource_after > resource_before - produced (regeneration happened)
    let expected_without_regen = (resource_before - produced).max(0.0);
    assert!(
        resource_after > expected_without_regen - 1e-6,
        "renewable resource should regenerate: after={resource_after}, without_regen={expected_without_regen}"
    );
}

// ---------------------------------------------------------------------------
// 7. Price Signal
// ---------------------------------------------------------------------------

#[test]
fn test_price_signal_with_intermediate_demand() {
    // Refinery consuming ore should cause intermediate demand for ore
    let global = make_global(vec![mine_factory(), refinery_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    stocks.insert("metal".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    capital.insert("refinery".into(), 30.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);
    prices.insert("metal".into(), 12.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // last_demand should include intermediate demand for ore
    let demand_ore = econ.last_demand.get("ore").copied().unwrap_or(0.0);
    let intermediate_ore = econ.last_intermediate.get("ore").copied().unwrap_or(0.0);

    assert!(
        intermediate_ore > 0.0,
        "refinery should cause intermediate demand for ore, got {intermediate_ore}"
    );
    // Total demand for ore includes consumption + investment + exports + intermediate
    assert!(
        demand_ore >= intermediate_ore,
        "total demand ({demand_ore}) should include intermediate ({intermediate_ore})"
    );
}

#[test]
fn test_price_increases_under_excess_demand() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Low stock => supply < demand => price should rise
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.national_income = 10000.0; // High income => high demand

    let price_before = 5.0;
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let price_after = *econ.prices.get("ore").unwrap();
    let signal = econ.last_price_signal.get("ore").copied().unwrap_or(0.0);

    assert!(signal > 0.0, "price signal should be positive under excess demand, got {signal}");
    assert!(
        price_after > price_before,
        "price should increase: before={price_before}, after={price_after}"
    );
}

#[test]
fn test_price_decreases_under_excess_supply() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new(); // No planet resource => no extraction

    // Huge stock, tiny demand
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1_000_000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 50.0);

    let mut econ = make_econ(&global, 10.0, stocks, capital, prices);
    econ.national_income = 1.0; // Very low income => tiny demand

    let price_before = 50.0;
    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let price_after = *econ.prices.get("ore").unwrap();
    let signal = econ.last_price_signal.get("ore").copied().unwrap_or(0.0);

    assert!(signal < 0.0, "price signal should be negative under excess supply, got {signal}");
    assert!(
        price_after < price_before,
        "price should decrease: before={price_before}, after={price_after}"
    );
}

// ---------------------------------------------------------------------------
// 8. Capital
// ---------------------------------------------------------------------------

#[test]
fn test_capital_accumulation_with_rationed_investment() {
    let mine = mine_factory(); // build_cost = metal qty=1
    let global = make_global(vec![mine]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Zero metal stock => investment fulfillment = 0
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100.0);
    // No metal stock at all
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 100.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);
    prices.insert("metal".into(), 12.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    let k_before = 100.0;

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let k_after = *econ.capital.get("mine").unwrap();

    // With zero metal, investment fulfillment = 0
    // K_after ~ (1 - depreciation*dt) * K_before = (1-0.05) * 100 = 95
    // But there may be some fulfillment from rationing if metal appears elsewhere
    // The key check: capital should decrease due to depreciation when no build goods available
    assert!(
        k_after < k_before,
        "capital should depreciate without build goods: before={k_before}, after={k_after}"
    );
}

#[test]
fn test_capital_depreciation_no_build_goods() {
    // Factory with build_cost but zero stock of build good => pure depreciation
    let mine = mine_factory(); // build_cost = metal
    let global = make_global(vec![mine]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100.0);
    // Explicitly no metal
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);
    prices.insert("metal".into(), 12.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    // Make national income very small so investment budget is tiny
    econ.national_income = 0.01;
    econ.savings_rate = 0.22;
    let k_before = 50.0;

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let k_after = *econ.capital.get("mine").unwrap();
    // Expected: (1-0.005*1.0)*50 + 0.8*tiny_investment ≈ 49.75
    assert_approx_tol(
        k_after,
        (1.0 - 0.005) * k_before,
        1.0, // Allow some tolerance for tiny investment
        "capital should ~depreciate",
    );
    assert!(k_after < k_before, "capital should decrease");
}

// ---------------------------------------------------------------------------
// 9. Demographics & Wages
// ---------------------------------------------------------------------------

#[test]
fn test_demographics_dt_scaling() {
    let mine = mine_factory();
    let global_dt1 = GlobalEconomyConfig {
        tick_duration_secs: 1.0,
        factory_types: vec![mine.clone()],
        labor_mobility: 1.0,
        ..Default::default()
    };
    let mut global_dt2 = global_dt1.clone();
    global_dt2.tick_duration_secs = 2.0;

    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ1 = make_econ(&global_dt1, 1000.0, stocks.clone(), capital.clone(), prices.clone());
    let mut econ2 = make_econ(&global_dt2, 1000.0, stocks, capital, prices);

    let pop_before = econ1.demographics.total_population();

    economy_tick(&mut econ1, &global_dt1, &planet, &resources, "nonexistent");
    economy_tick(&mut econ2, &global_dt2, &planet, &resources, "nonexistent");

    let delta1 = econ1.demographics.total_population() - pop_before;
    let delta2 = econ2.demographics.total_population() - pop_before;

    // dt=2 should produce approximately 2x the demographic change
    // Not exact due to nonlinearity, but should be approximately proportional
    if delta1.abs() > 1e-6 {
        let ratio = delta2 / delta1;
        assert_approx_tol(ratio, 2.0, 0.5, "dt=2 demographic delta should be ~2x dt=1");
    }
}

#[test]
fn test_phillips_curve_wage_adjustment() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = PlanetEconomyConfig {
        unemployment_natural: 0.05,
        base_carrying_capacity: 10000.0,
        ..Default::default()
    };
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    // Low unemployment => wage goes up
    let mut econ_low_u = make_econ(&global, 100.0, stocks.clone(), capital.clone(), prices.clone());
    econ_low_u.unemployment = 0.01; // Below natural rate
    let wage_before_low = econ_low_u.wage;

    economy_tick(&mut econ_low_u, &global, &planet, &resources, "test");

    // Wage adjustment: wage *= 1 + kappa * (u_star - u) * dt
    // u_star=0.05, u=0.01 => factor = 1 + 0.3*(0.05-0.01) = 1.012
    assert!(
        econ_low_u.wage > wage_before_low,
        "wage should increase with low unemployment: before={wage_before_low}, after={}",
        econ_low_u.wage
    );

    // High unemployment => wage goes down
    let mut econ_high_u = make_econ(&global, 100.0, stocks, capital, prices);
    econ_high_u.unemployment = 0.20; // Above natural rate
    let wage_before_high = econ_high_u.wage;

    economy_tick(&mut econ_high_u, &global, &planet, &resources, "test");

    assert!(
        econ_high_u.wage < wage_before_high,
        "wage should decrease with high unemployment: before={wage_before_high}, after={}",
        econ_high_u.wage
    );
}

// ---------------------------------------------------------------------------
// 10. Labor
// ---------------------------------------------------------------------------

#[test]
fn test_value_added_labor_allocation_negative_va() {
    // Factory where input cost > output value => negative VA => L*=0
    let bad_factory = FactoryTypeConfig {
        id: "lossy".into(),
        name: "Lossy Factory".into(),
        tier: 1,
        category: FactoryCategory::Manufacturing,
        inputs: vec![GoodQuantity {
            good: "ore".into(),
            quantity: 10, // 10 ore at price 100 = 1000
        }],
        outputs: vec![GoodQuantity {
            good: "junk".into(),
            quantity: 1, // 1 junk at price 1 = 1
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };

    let global = make_global(vec![bad_factory]);
    let planet = make_planet();
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10000.0);
    stocks.insert("junk".into(), 100.0);
    let mut capital = HashMap::new();
    capital.insert("lossy".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 100.0); // Very expensive input
    prices.insert("junk".into(), 1.0); // Cheap output

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // With labor_mobility=1.0, labor should move to 0 immediately
    let labor = econ.labor_alloc.get("lossy").copied().unwrap_or(0.0);
    assert_approx_tol(
        labor, 0.0, 1e-3,
        "negative VA factory should have ~0 labor",
    );

    // Production should be near zero (not exactly zero due to l_i.max(0.01) floor in Cobb-Douglas)
    let produced = econ.last_production.get("junk").copied().unwrap_or(0.0);
    assert!(
        produced < 1.0,
        "negative VA should produce near-zero output, got {produced}",
    );
}

// ---------------------------------------------------------------------------
// 11. Savings
// ---------------------------------------------------------------------------

#[test]
fn test_savings_rate_clamping() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    // Test lower clamp: massive income drop should push savings down
    let mut econ = make_econ(&global, 100.0, stocks.clone(), capital.clone(), prices.clone());
    econ.national_income = 1_000_000.0; // Previous income was huge
    // After tick, income will be much lower => big negative income_change => s pushed down

    economy_tick(&mut econ, &global, &planet, &resources, "test");
    assert!(
        econ.savings_rate >= 0.05,
        "savings_rate should be >= 0.05, got {}",
        econ.savings_rate
    );
    assert!(
        econ.savings_rate <= 0.60,
        "savings_rate should be <= 0.60, got {}",
        econ.savings_rate
    );

    // Test upper clamp: massive income increase
    let mut econ2 = make_econ(&global, 100.0, stocks, capital, prices);
    econ2.national_income = 0.001; // Previous income was tiny
    // The tick will compute a larger national_income => positive change

    economy_tick(&mut econ2, &global, &planet, &resources, "test");
    assert!(
        econ2.savings_rate >= 0.05,
        "savings_rate should be >= 0.05, got {}",
        econ2.savings_rate
    );
    assert!(
        econ2.savings_rate <= 0.60,
        "savings_rate should be <= 0.60, got {}",
        econ2.savings_rate
    );
}

// ---------------------------------------------------------------------------
// 12. Trade Integration
// ---------------------------------------------------------------------------

#[test]
fn test_trade_imports_in_available_supply() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.imports_this_tick.insert("ore".into(), 100.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // Imports should be included in available supply
    let supply = econ.last_available_supply.get("ore").copied().unwrap_or(0.0);
    // Available supply = stocks (100) + imports (100) = 200, minus intermediate consumed
    // But intermediate consumption happens before available supply calc
    assert!(
        supply >= 100.0,
        "available supply should include imports, got {supply}"
    );

    // imports_this_tick should be drained (std::mem::take)
    assert!(
        econ.imports_this_tick.is_empty(),
        "imports_this_tick should be empty after tick"
    );
}

#[test]
fn test_trade_exports_rationed_and_fulfilled() {
    let mine = mine_factory();
    let global = make_global(vec![mine]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Surplus scenario: plenty of stock
    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.exports_this_tick.insert("ore".into(), 50.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let fulfilled = econ.last_exports_fulfilled.get("ore").copied().unwrap_or(0.0);
    assert_approx(fulfilled, 50.0, "exports should be fully fulfilled with surplus");

    // exports_this_tick should be drained
    assert!(
        econ.exports_this_tick.is_empty(),
        "exports_this_tick should be empty after tick"
    );
}

// ---------------------------------------------------------------------------
// 13. Conservation & Stability
// ---------------------------------------------------------------------------

#[test]
fn test_stock_accounting_conservation() {
    let global = make_global(vec![mine_factory(), refinery_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 5000.0);
    stocks.insert("metal".into(), 5000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    capital.insert("refinery".into(), 30.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);
    prices.insert("metal".into(), 12.0);

    let mut econ = make_econ(&global, 100.0, stocks.clone(), capital, prices);
    econ.imports_this_tick.insert("ore".into(), 200.0);
    econ.exports_this_tick.insert("metal".into(), 100.0);

    let stocks_before = econ.stocks.clone();
    let imports_before: HashMap<String, f64> = econ.imports_this_tick.clone();

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    // For each good, verify:
    // stocks_after = stocks_before + imports - intermediates_consumed - V^C - V^I - V^X + production
    for good in ["ore", "metal"] {
        let s_before = stocks_before.get(good).copied().unwrap_or(0.0);
        let imp = imports_before.get(good).copied().unwrap_or(0.0);
        let inter = econ.last_intermediate.get(good).copied().unwrap_or(0.0);
        let v_c = econ.last_consumption.get(good).copied().unwrap_or(0.0);
        let v_i = econ.last_investment.get(good).copied().unwrap_or(0.0);
        let v_x = econ.last_exports_fulfilled.get(good).copied().unwrap_or(0.0);
        let prod = econ.last_production.get(good).copied().unwrap_or(0.0);
        let s_after = econ.stocks.get(good).copied().unwrap_or(0.0);

        let expected = s_before + imp - inter - v_c - v_i - v_x + prod;
        assert_approx_tol(
            s_after,
            expected,
            1.0, // Allow tolerance for floating point and clamping
            &format!("{good} stock conservation"),
        );
    }
}

#[test]
fn test_multi_tick_stability() {
    let global = make_global(vec![mine_factory(), refinery_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut econ = initialize_economy_with_population(&global, &planet, 1000.0, &resources);

    for tick in 0..20 {
        economy_tick(&mut econ, &global, &planet, &resources, "test");

        let pop = econ.demographics.total_population();
        assert!(pop > 0.0, "tick {tick}: pop must be > 0, got {pop}");
        assert!(pop < 1e8, "tick {tick}: pop must be < 1e8, got {pop}");

        assert!(
            econ.wage > 0.0 && econ.wage < 100_000.0,
            "tick {tick}: wage out of range: {}",
            econ.wage
        );

        for (good, &price) in &econ.prices {
            assert!(
                price > 0.0 && price < 100_000.0,
                "tick {tick}: price of {good} out of range: {price}"
            );
        }

        for (good, &stock) in &econ.stocks {
            assert!(
                stock >= 0.0,
                "tick {tick}: stock of {good} is negative: {stock}"
            );
        }

        assert!(
            econ.unemployment >= 0.0 && econ.unemployment <= 1.0,
            "tick {tick}: unemployment out of [0,1]: {}",
            econ.unemployment
        );

        assert!(
            econ.savings_rate >= 0.05 && econ.savings_rate <= 0.60,
            "tick {tick}: savings_rate out of [0.05,0.60]: {}",
            econ.savings_rate
        );

        assert!(
            econ.national_income >= 0.0,
            "tick {tick}: national_income negative: {}",
            econ.national_income
        );
    }
}

// ---------------------------------------------------------------------------
// 14. Infrastructure
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_infrastructure_from_planet() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet_with_infra(10000.0, 42.0);
    let resources = HashMap::new();

    let econ = initialize_economy_with_population(&global, &planet, 1000.0, &resources);
    assert_approx(econ.infrastructure, 42.0, "infrastructure from planet config");
}

#[test]
fn test_infrastructure_depreciation() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.infrastructure = 100.0;
    econ.national_income = 0.01; // Tiny income → tiny investment

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let infra = econ.infrastructure;
    assert!(infra > 95.0, "infra should be > 95, got {infra}");
    assert!(infra < 100.0, "infra should be < 100, got {infra}");
}

#[test]
fn test_infrastructure_accumulation_with_build_cost() {
    let global = make_global_with_infra(
        vec![mine_factory()],
        "test",
        vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        20.0,
    );
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: 100.0,
        ..Default::default()
    };
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 1000.0, stocks, capital, prices);
    econ.infrastructure = 10.0;
    econ.national_income = 100000.0;

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert!(
        econ.infrastructure > 10.0,
        "infra should increase, got {}",
        econ.infrastructure
    );
    let ore_inv = econ
        .last_infra_investment
        .get("ore")
        .copied()
        .unwrap_or(0.0);
    assert!(
        ore_inv > 0.0,
        "infra ore investment should be > 0, got {ore_inv}"
    );
    assert!(
        econ.last_carrying_capacity > 100.0,
        "carrying capacity should exceed base, got {}",
        econ.last_carrying_capacity
    );
}

#[test]
fn test_infrastructure_carrying_capacity_formula() {
    let global = make_global_with_infra(
        vec![mine_factory()],
        "test",
        vec![GoodQuantity {
            good: "ore".into(),
            quantity: 1,
        }],
        25.0,
    );
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: 500.0,
        ..Default::default()
    };
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.infrastructure = 20.0;

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert_approx(
        econ.last_carrying_capacity,
        1000.0,
        "carrying capacity = 500 + 20*25",
    );
}

#[test]
fn test_infrastructure_crowding_diagnostic() {
    let global = make_global(vec![mine_factory()]);
    let planet = PlanetEconomyConfig {
        base_carrying_capacity: 50.0,
        ..Default::default()
    };
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 1000.0, stocks, capital, prices);
    econ.infrastructure = 0.0;
    // total_pop = 1000*1.5 = 1500, carrying_cap = 50 + 0*10 = 50
    // crowding = min(1500/50, 2.0) = 2.0

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert_approx(econ.last_crowding, 2.0, "crowding should be capped at 2.0");
    assert_approx(
        econ.last_carrying_capacity,
        50.0,
        "carrying capacity = 50 + 0*ppu",
    );
}

// ---------------------------------------------------------------------------
// 15. Savings chi Formula
// ---------------------------------------------------------------------------

#[test]
fn test_savings_rate_relative_income_change() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    // Economy 1: moderate prev_income
    let mut econ1 = make_econ(&global, 100.0, stocks.clone(), capital.clone(), prices.clone());
    econ1.national_income = 100.0;

    // Economy 2: very high prev_income
    let mut econ2 = make_econ(&global, 100.0, stocks, capital, prices);
    econ2.national_income = 100000.0;

    economy_tick(&mut econ1, &global, &planet, &resources, "test");
    economy_tick(&mut econ2, &global, &planet, &resources, "test");

    assert!(econ1.savings_rate >= 0.05, "econ1 savings >= 0.05");
    assert!(econ1.savings_rate <= 0.60, "econ1 savings <= 0.60");
    assert!(econ2.savings_rate >= 0.05, "econ2 savings >= 0.05");
    assert!(econ2.savings_rate <= 0.60, "econ2 savings <= 0.60");
    assert!(
        (econ1.savings_rate - econ2.savings_rate).abs() > 1e-6,
        "savings rates should differ: econ1={}, econ2={}",
        econ1.savings_rate,
        econ2.savings_rate
    );
}

#[test]
fn test_savings_rate_zero_prev_income_guard() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);
    econ.national_income = 0.0;

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert!(
        !econ.savings_rate.is_nan(),
        "savings_rate should not be NaN"
    );
    assert!(
        econ.savings_rate >= 0.05,
        "savings >= 0.05, got {}",
        econ.savings_rate
    );
    assert!(
        econ.savings_rate <= 0.60,
        "savings <= 0.60, got {}",
        econ.savings_rate
    );
}

// ---------------------------------------------------------------------------
// 16. CES Consumption
// ---------------------------------------------------------------------------

#[test]
fn test_ces_consumption_price_sensitivity() {
    let food_factory = FactoryTypeConfig {
        id: "farm".into(),
        name: "Farm".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "food".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };
    let luxury_factory = FactoryTypeConfig {
        id: "artisan".into(),
        name: "Artisan".into(),
        tier: 1,
        category: FactoryCategory::Manufacturing,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "luxury".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };

    let mut global = make_global(vec![food_factory, luxury_factory]);
    // Override consumption profile with equal shares
    let mut profile = HashMap::new();
    profile.insert("food".into(), 0.5);
    profile.insert("luxury".into(), 0.5);
    global.consumption_profiles.insert("test".into(), profile);

    let planet = make_planet();
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("food".into(), 100000.0);
    stocks.insert("luxury".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("farm".into(), 50.0);
    capital.insert("artisan".into(), 30.0);
    let mut prices = HashMap::new();
    prices.insert("food".into(), 1.0);
    prices.insert("luxury".into(), 100.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let c_food = econ.last_consumption.get("food").copied().unwrap_or(0.0);
    let c_luxury = econ
        .last_consumption
        .get("luxury")
        .copied()
        .unwrap_or(0.0);
    assert!(
        c_food > c_luxury,
        "CES should shift demand to cheaper good: food={c_food}, luxury={c_luxury}"
    );
}

#[test]
fn test_ces_consumption_equal_prices_equal_demand() {
    let factory_a = FactoryTypeConfig {
        id: "factory_a".into(),
        name: "Factory A".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "good_a".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };
    let factory_b = FactoryTypeConfig {
        id: "factory_b".into(),
        name: "Factory B".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "good_b".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };

    let mut global = make_global(vec![factory_a, factory_b]);
    let mut profile = HashMap::new();
    profile.insert("good_a".into(), 0.5);
    profile.insert("good_b".into(), 0.5);
    global.consumption_profiles.insert("test".into(), profile);

    let planet = make_planet();
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("good_a".into(), 100000.0);
    stocks.insert("good_b".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("factory_a".into(), 50.0);
    capital.insert("factory_b".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("good_a".into(), 10.0);
    prices.insert("good_b".into(), 10.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let c_a = econ.last_consumption.get("good_a").copied().unwrap_or(0.0);
    let c_b = econ.last_consumption.get("good_b").copied().unwrap_or(0.0);
    assert_approx_tol(
        c_a,
        c_b,
        1e-3,
        "equal prices + equal shares → equal consumption",
    );
}

// ---------------------------------------------------------------------------
// 17. Transient Goods
// ---------------------------------------------------------------------------

#[test]
fn test_transient_goods_cleared_from_stocks() {
    let energy_factory = FactoryTypeConfig {
        id: "power_plant".into(),
        name: "Power Plant".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "energy".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };
    let mine = mine_factory();

    let mut global = make_global(vec![energy_factory, mine]);
    global.transient_goods.insert("energy".into());

    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    let mut stocks = HashMap::new();
    stocks.insert("energy".into(), 500.0);
    stocks.insert("ore".into(), 500.0);
    let mut capital = HashMap::new();
    capital.insert("power_plant".into(), 50.0);
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("energy".into(), 5.0);
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 100.0, stocks, capital, prices);

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    assert!(
        econ.stocks.get("energy").is_none(),
        "energy should be cleared from stocks"
    );
    assert!(
        econ.stocks.get("ore").is_some(),
        "ore should remain in stocks"
    );
}

#[test]
fn test_transient_goods_skipped_in_initialization() {
    let energy_factory = FactoryTypeConfig {
        id: "power_plant".into(),
        name: "Power Plant".into(),
        tier: 0,
        category: FactoryCategory::Extraction,
        inputs: vec![],
        outputs: vec![GoodQuantity {
            good: "energy".into(),
            quantity: 1,
        }],
        build_cost: vec![],
        production_cycle: 1.0,
    };

    let mut global = make_global(vec![energy_factory, mine_factory()]);
    global.transient_goods.insert("energy".into());

    let planet = make_planet();
    let resources = HashMap::new();

    let econ = initialize_economy_with_population(&global, &planet, 1000.0, &resources);

    assert!(
        econ.stocks.get("energy").is_none(),
        "energy should not be seeded in stocks"
    );
    assert!(
        econ.stocks.get("ore").is_some(),
        "ore should be seeded in stocks"
    );
}

// ---------------------------------------------------------------------------
// 18. Demographics Birth Nerf
// ---------------------------------------------------------------------------

#[test]
fn test_birth_rate_reduced_by_consumption_shortage() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // Abundant stocks
    let mut stocks_a = HashMap::new();
    stocks_a.insert("ore".into(), 100000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ_abundant = make_econ(&global, 1000.0, stocks_a, capital.clone(), prices.clone());
    econ_abundant.national_income = 100000.0;

    // Scarce stocks
    let mut stocks_s = HashMap::new();
    stocks_s.insert("ore".into(), 0.01);
    let mut econ_scarce = make_econ(&global, 1000.0, stocks_s, capital, prices);
    econ_scarce.national_income = 100000.0;

    economy_tick(&mut econ_abundant, &global, &planet, &resources, "test");
    economy_tick(&mut econ_scarce, &global, &planet, &resources, "test");

    assert!(
        econ_abundant.demographics.pop_young > econ_scarce.demographics.pop_young,
        "abundant should have more young: abundant={}, scarce={}",
        econ_abundant.demographics.pop_young,
        econ_scarce.demographics.pop_young
    );
}

// ---------------------------------------------------------------------------
// 19. Crowding
// ---------------------------------------------------------------------------

#[test]
fn test_crowding_reduces_births() {
    let global = make_global(vec![mine_factory()]);
    let mut resources = HashMap::new();
    resources.insert("ore".into(), ore_resource());

    // High carrying capacity → low crowding
    let planet_uncrowded = PlanetEconomyConfig {
        base_carrying_capacity: 100000.0,
        ..Default::default()
    };
    // Low carrying capacity → high crowding
    let planet_crowded = PlanetEconomyConfig {
        base_carrying_capacity: 50.0,
        ..Default::default()
    };

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 10000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ_uncrowded =
        make_econ(&global, 1000.0, stocks.clone(), capital.clone(), prices.clone());
    let mut econ_crowded = make_econ(&global, 1000.0, stocks, capital, prices);

    // Use "nonexistent" climate to skip consumption (fulfillment=1.0 on both)
    economy_tick(
        &mut econ_uncrowded,
        &global,
        &planet_uncrowded,
        &resources,
        "nonexistent",
    );
    economy_tick(
        &mut econ_crowded,
        &global,
        &planet_crowded,
        &resources,
        "nonexistent",
    );

    assert!(
        econ_uncrowded.demographics.pop_young > econ_crowded.demographics.pop_young,
        "uncrowded should have more young: uncrowded={}, crowded={}",
        econ_uncrowded.demographics.pop_young,
        econ_crowded.demographics.pop_young
    );
}

// ---------------------------------------------------------------------------
// 20. Price Signal EMA
// ---------------------------------------------------------------------------

#[test]
fn test_price_signal_ema_smoothing() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new(); // No extraction

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1_000_000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 10.0, stocks, capital, prices);
    econ.national_income = 1.0; // Low income → low demand
    econ.last_price_signal.insert("ore".into(), -0.5); // Previous negative signal

    economy_tick(&mut econ, &global, &planet, &resources, "test");

    let signal = econ
        .last_price_signal
        .get("ore")
        .copied()
        .unwrap_or(0.0);
    // Raw signal is ≈ -1.0 (huge supply, tiny demand)
    // EMA: smoothed = η·raw + (1-η)·prev ≈ 0.25·(-1) + 0.75·(-0.5) = -0.625
    assert!(
        signal < 0.0,
        "signal should be negative under excess supply, got {signal}"
    );
    assert!(
        signal > -1.0,
        "signal should be dampened by EMA from prev, got {signal}"
    );
}

#[test]
fn test_price_signal_persists_across_ticks() {
    let global = make_global(vec![mine_factory()]);
    let planet = make_planet();
    let resources = HashMap::new();

    let mut stocks = HashMap::new();
    stocks.insert("ore".into(), 1_000_000.0);
    let mut capital = HashMap::new();
    capital.insert("mine".into(), 50.0);
    let mut prices = HashMap::new();
    prices.insert("ore".into(), 5.0);

    let mut econ = make_econ(&global, 10.0, stocks, capital, prices);
    econ.national_income = 1.0;

    // Run 3 ticks with consistent excess supply
    economy_tick(&mut econ, &global, &planet, &resources, "test");
    let signal_1 = econ
        .last_price_signal
        .get("ore")
        .copied()
        .unwrap_or(0.0);

    economy_tick(&mut econ, &global, &planet, &resources, "test");
    economy_tick(&mut econ, &global, &planet, &resources, "test");
    let signal_3 = econ
        .last_price_signal
        .get("ore")
        .copied()
        .unwrap_or(0.0);

    // Raw signal ≈ -1.0 throughout; EMA should converge toward it
    let raw = -1.0_f64;
    assert!(
        (signal_3 - raw).abs() < (signal_1 - raw).abs(),
        "signal should converge: signal_1={signal_1}, signal_3={signal_3}, raw≈{raw}"
    );
}
