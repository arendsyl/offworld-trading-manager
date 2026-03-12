use std::collections::HashMap;

use crate::economy::config::{
    GlobalEconomyConfig, PlanetEconomyConfig, default_people_per_infra_unit,
};
use crate::economy::models::{Demographics, EconomyState};
use crate::models::PlanetResource;

/// Initialize an economy from config defaults (no planet resources).
pub fn initialize_economy(
    global: &GlobalEconomyConfig,
    planet: &PlanetEconomyConfig,
    planet_resources: &HashMap<String, PlanetResource>,
) -> EconomyState {
    initialize_economy_with_population(global, planet, planet.initial_pop_active, planet_resources)
}

/// Initialize an economy with a given population and planet resources.
pub fn initialize_economy_with_population(
    global: &GlobalEconomyConfig,
    planet: &PlanetEconomyConfig,
    pop: f64,
    planet_resources: &HashMap<String, PlanetResource>,
) -> EconomyState {
    let pop = pop.max(1.0);

    // Distribute population: 20% young, 65% active, 15% old
    let demographics = Demographics {
        pop_young: pop * 0.20,
        pop_active: pop * 0.65,
        pop_old: pop * 0.15,
    };

    // Use planet overrides if provided, otherwise build from factory types
    // Scale default capital proportionally to population (defaults calibrated for 100 active)
    let capital = if planet.initial_capital.is_empty() {
        let mut cap = crate::economy::config::build_default_initial_capital(&global.factory_types);
        let reference_pop = 100.0_f64;
        let scale = (pop * 0.65 / reference_pop).max(1.0); // 0.65 = active fraction
        for v in cap.values_mut() {
            *v *= scale;
        }
        cap
    } else {
        planet.initial_capital.clone()
    };
    let prices = if planet.initial_prices.is_empty() {
        crate::economy::config::build_default_initial_prices(&global.factory_types)
    } else {
        planet.initial_prices.clone()
    };

    // Initialize resource stocks to max_capacity from planet resources
    let resources: HashMap<String, f64> = planet_resources
        .iter()
        .map(|(id, pr)| (id.clone(), pr.max_capacity))
        .collect();

    // Compute VA per factory at initial prices
    let mut va_init: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let va: f64 = (factory
            .outputs
            .iter()
            .map(|gq| gq.quantity as f64 * prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum::<f64>()
            - factory
                .inputs
                .iter()
                .map(|gq| gq.quantity as f64 * prices.get(&gq.good).copied().unwrap_or(1.0))
                .sum::<f64>())
            / factory.production_cycle;
        va_init.insert(factory.id.clone(), va);
    }

    // Allocate labor proportional to VA share
    let target_employed = demographics.pop_active * (1.0 - planet.unemployment_natural);
    let attractiveness: HashMap<String, f64> = va_init
        .iter()
        .map(|(id, &va)| (id.clone(), va.max(0.0)))
        .collect();
    let total_attractiveness: f64 = attractiveness.values().sum();

    let mut labor_alloc: HashMap<String, f64> = HashMap::new();
    if total_attractiveness > 0.0 {
        for factory in &global.factory_types {
            let a_i = attractiveness.get(&factory.id).copied().unwrap_or(0.0);
            labor_alloc.insert(
                factory.id.clone(),
                target_employed * a_i / total_attractiveness,
            );
        }
    } else {
        // All factories have non-positive VA — no factory is worth running
        for factory in &global.factory_types {
            labor_alloc.insert(factory.id.clone(), 0.0);
        }
    }

    // Initialize goods stockpile: seed with enough buffer for economy to stabilize (skip transient)
    let mut stocks: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        for output in &factory.outputs {
            if !global.transient_goods.contains(&output.good) {
                stocks.entry(output.good.clone()).or_insert(pop * 5.0);
            }
        }
    }

    // Estimate init_income from extractors only (they can produce on tick 1
    // without intermediates; higher-tier factories contribute in later ticks)
    let mut init_income = 0.0_f64;
    for factory in &global.factory_types {
        if !factory.inputs.is_empty() {
            continue;
        }
        let k_i = capital.get(&factory.id).copied().unwrap_or(0.01).max(0.01);
        let l_i = labor_alloc.get(&factory.id).copied().unwrap_or(0.01).max(0.01);
        let mut y = k_i.powf(global.alpha) * l_i.powf(global.beta) / factory.production_cycle;

        if factory.is_extraction() {
            for output in &factory.outputs {
                if let Some(pr) = planet_resources.get(&output.good) {
                    let cap_sat = k_i / (k_i + pr.k_half.max(0.01));
                    let limit = pr.max_extraction * cap_sat / factory.production_cycle;
                    y = y.min(limit);
                }
            }
        }

        let revenue: f64 = factory.outputs.iter()
            .map(|gq| gq.quantity as f64 * y * prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum();
        init_income += revenue;
    }

    EconomyState {
        demographics,
        capital,
        labor_alloc,
        prices,
        wage: planet.initial_wage,
        unemployment: planet.unemployment_natural,
        national_income: init_income,
        savings_rate: global.s_0,
        resources,
        stocks,
        prev_wage: planet.initial_wage,
        infrastructure: planet.initial_infrastructure,
        initialized: true,
        ..Default::default()
    }
}

/// Run one economy tick. Reads t-1 state from `econ`, computes t, writes results back.
pub fn economy_tick(
    econ: &mut EconomyState,
    global: &GlobalEconomyConfig,
    planet: &PlanetEconomyConfig,
    planet_resources: &HashMap<String, PlanetResource>,
    climate_key: &str,
) {
    if !econ.initialized {
        return;
    }

    let dt = global.tick_duration_secs;
    let prev_income = econ.national_income;

    // --- 1. Capture labor supply (explicit Euler — demographics move at end) ---
    let labor_supply = econ.demographics.pop_active;

    // --- 2. Unemployment & Wages (Phillips curve, dt-scaled) ---
    let u_star = planet.unemployment_natural;
    let prev_wage_snapshot = econ.wage;
    econ.wage *= 1.0 + global.phillips_kappa * (u_star - econ.unemployment) * dt;
    econ.wage = econ.wage.clamp(0.01, 100_000.0);
    let wage_change = if prev_wage_snapshot > 0.0 {
        (econ.wage - prev_wage_snapshot) / prev_wage_snapshot
    } else {
        0.0
    };

    // --- 3. VA-Proportional Labor Allocation ---
    let mut va_per_factory: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let va_i: f64 = (factory
            .outputs
            .iter()
            .map(|gq| gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum::<f64>()
            - factory
                .inputs
                .iter()
                .map(|gq| gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
                .sum::<f64>())
            / factory.production_cycle;
        va_per_factory.insert(factory.id.clone(), va_i);
    }

    // Compute attractiveness per factory (capital-weighted VA)
    let attractiveness: HashMap<String, f64> = va_per_factory
        .iter()
        .map(|(id, &va)| {
            let k_i = econ.capital.get(id).copied().unwrap_or(0.0);
            (id.clone(), k_i.powf(global.alpha) * va.max(0.0))
        })
        .collect();
    let total_attractiveness: f64 = attractiveness.values().sum();

    // Target: allocate (1 - u*) of labor_supply proportionally to attractiveness
    let target_employed = labor_supply * (1.0 - planet.unemployment_natural);
    let mut l_target: HashMap<String, f64> = HashMap::new();
    if total_attractiveness > 0.0 {
        for factory in &global.factory_types {
            let a_i = attractiveness.get(&factory.id).copied().unwrap_or(0.0);
            l_target.insert(
                factory.id.clone(),
                target_employed * a_i / total_attractiveness,
            );
        }
    } else {
        // All factories have non-positive VA — no factory is worth running
        for factory in &global.factory_types {
            l_target.insert(factory.id.clone(), 0.0);
        }
    }

    // Partial adjustment: blend current allocation toward target
    for factory in &global.factory_types {
        let l_i = econ.labor_alloc.get(&factory.id).copied().unwrap_or(0.0);
        let target = l_target.get(&factory.id).copied().unwrap_or(0.0);
        let new_l = (1.0 - global.labor_mobility * dt) * l_i + global.labor_mobility * dt * target;
        econ.labor_alloc.insert(factory.id.clone(), new_l.max(0.0));
    }

    // Cap total allocation to labor supply (no phantom labor)
    let total_employed: f64 = econ.labor_alloc.values().sum();
    if total_employed > labor_supply {
        let scale = labor_supply / total_employed;
        for v in econ.labor_alloc.values_mut() {
            *v *= scale;
        }
    }

    // Unemployment from actual allocation (fluctuates around u*)
    let total_employed: f64 = econ.labor_alloc.values().sum();
    econ.unemployment = if labor_supply > 0.0 {
        (1.0 - total_employed / labor_supply).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // --- 4. Production (one-tick delay, input rationing across factories) ---
    let mut factory_runs: HashMap<String, f64> = HashMap::new();
    let mut production_per_good: HashMap<String, f64> = HashMap::new();
    let mut pending_production: HashMap<String, f64> = HashMap::new();

    // Michaelis-Menten extraction limits for tier-0 extractors
    let mut extraction_limits: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        if factory.is_extraction() {
            for output in &factory.outputs {
                if let (Some(&stock), Some(pr)) = (
                    econ.resources.get(&output.good),
                    planet_resources.get(&output.good),
                ) {
                    let k_i = econ.capital.get(&factory.id).copied().unwrap_or(0.0);
                    let stock_ratio = stock / pr.max_capacity.max(1.0);
                    let capital_saturation = k_i / (k_i + pr.k_half.max(0.01));
                    let limit = pr.max_extraction * capital_saturation * stock_ratio;
                    extraction_limits.insert(factory.id.clone(), limit);
                }
            }
        }
    }

    // Snapshot start-of-tick stocks for input availability
    let stocks_snapshot = econ.stocks.clone();

    // Y_pot: unconstrained Cobb-Douglas capacity (with extraction cap for tier-0)
    let mut y_pot: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let k_i = econ
            .capital
            .get(&factory.id)
            .copied()
            .unwrap_or(0.0)
            .max(0.01);
        let l_i = econ
            .labor_alloc
            .get(&factory.id)
            .copied()
            .unwrap_or(0.0)
            .max(0.01);
        let y_cobb = k_i.powf(global.alpha) * l_i.powf(global.beta);

        let y_cap = if factory.is_extraction() {
            y_cobb.min(extraction_limits.get(&factory.id).copied().unwrap_or(0.0))
        } else {
            y_cobb
        }
        .max(0.0)
            / factory.production_cycle;

        y_pot.insert(factory.id.clone(), y_cap);
    }

    // Y_io: Leontief input constraint per factory using snapshot
    let mut y_io: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        if factory.inputs.is_empty() {
            y_io.insert(factory.id.clone(), f64::INFINITY);
            continue;
        }
        let min_ratio = factory
            .inputs
            .iter()
            .map(|gq| {
                let available = stocks_snapshot.get(&gq.good).copied().unwrap_or(0.0);
                if gq.quantity > 0 {
                    available / gq.quantity as f64
                } else {
                    f64::INFINITY
                }
            })
            .fold(f64::INFINITY, f64::min);
        y_io.insert(
            factory.id.clone(),
            min_ratio.max(0.0) / factory.production_cycle,
        );
    }

    // Y_i = min(Y_pot, Y_io) per factory
    let mut y_unconstrained: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let yp = y_pot.get(&factory.id).copied().unwrap_or(0.0);
        let yi = y_io.get(&factory.id).copied().unwrap_or(0.0);
        y_unconstrained.insert(factory.id.clone(), yp.min(yi));
    }

    // Ration inputs across factories: compute total demand per good
    let mut total_input_demand: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        if factory.inputs.is_empty() {
            continue;
        }
        let y_i = y_unconstrained.get(&factory.id).copied().unwrap_or(0.0);
        for input in &factory.inputs {
            *total_input_demand.entry(input.good.clone()).or_default() +=
                input.quantity as f64 * y_i;
        }
    }

    // Compute rationing ratio per good
    let mut input_ratio: HashMap<String, f64> = HashMap::new();
    for (good, &demand) in &total_input_demand {
        let available = stocks_snapshot.get(good).copied().unwrap_or(0.0);
        let ratio = if demand > 0.0 {
            (available / demand).min(1.0)
        } else {
            1.0
        };
        input_ratio.insert(good.clone(), ratio);
    }

    // Scale each factory by its tightest bottleneck ratio
    for factory in &global.factory_types {
        let y_i_base = y_unconstrained.get(&factory.id).copied().unwrap_or(0.0);

        let y_i = if !factory.inputs.is_empty() {
            let bottleneck = factory
                .inputs
                .iter()
                .map(|gq| input_ratio.get(&gq.good).copied().unwrap_or(0.0))
                .fold(1.0_f64, f64::min);
            y_i_base * bottleneck
        } else {
            y_i_base
        }
        .max(0.0);

        factory_runs.insert(factory.id.clone(), y_i);

        for output in &factory.outputs {
            let produced = output.quantity as f64 * y_i * dt;
            *production_per_good.entry(output.good.clone()).or_default() += produced;
            *pending_production.entry(output.good.clone()).or_default() += produced;
        }
    }

    // Compute actual intermediate demand per good from factory_runs (NOT y_unconstrained)
    let mut d_m: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let y_i = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
        for input in &factory.inputs {
            *d_m.entry(input.good.clone()).or_default() += input.quantity as f64 * y_i * dt;
        }
    }

    // --- 5. Available Supply O_g (stocks + imports) ---
    let imports = std::mem::take(&mut econ.imports_this_tick);
    for (good, qty) in &imports {
        *econ.stocks.entry(good.clone()).or_insert(0.0) += qty;
    }
    let available_supply: HashMap<String, f64> = econ.stocks.clone();

    // --- 6. Desired Demand (no stock deductions) ---
    let s = econ.savings_rate;
    let consumption_total = (1.0 - s) * prev_income + global.mpc_wealth * econ.wealth;

    // D^C: CES consumption demand (desired, dt-scaled)
    let mut d_c: HashMap<String, f64> = HashMap::new();
    if let Some(shares) = global.consumption_profiles.get(climate_key) {
        let sigma = global.substitution_elasticity;

        let p_avg: f64 = shares
            .iter()
            .map(|(g, &alpha_g)| alpha_g * econ.prices.get(g).copied().unwrap_or(1.0))
            .sum::<f64>()
            .max(0.01);

        let mut omega: HashMap<String, f64> = HashMap::new();
        let mut omega_sum = 0.0_f64;
        for (good, &alpha_g) in shares {
            let p_g = econ.prices.get(good).copied().unwrap_or(1.0).max(0.01);
            let w = alpha_g * (p_g / p_avg).powf(sigma);
            omega_sum += w;
            omega.insert(good.clone(), w);
        }
        omega_sum = omega_sum.max(1e-10);

        for (good, w) in &omega {
            let w_norm = w / omega_sum;
            let p_g = econ.prices.get(good).copied().unwrap_or(1.0).max(0.01);
            let desired = consumption_total * w_norm / p_g * dt;
            d_c.insert(good.clone(), desired);
        }
    }

    // --- Carrying capacity and crowding (infrastructure-based) ---
    let total_pop = econ.demographics.total_population();
    let infra_cfg = global.infra_climate.get(climate_key);
    let ppu = infra_cfg
        .map(|c| c.people_per_unit)
        .unwrap_or(default_people_per_infra_unit());
    let carrying_cap = planet.base_carrying_capacity + econ.infrastructure * ppu;
    let crowding = (total_pop / carrying_cap.max(1.0)).min(2.0);

    // D^I: investment demand (desired, per good)
    let i_total = econ.savings_rate * econ.national_income;

    let mut profits: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let y_i = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
        let revenue: f64 = factory
            .outputs
            .iter()
            .map(|gq| gq.quantity as f64 * y_i * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum();
        let cost: f64 = factory
            .inputs
            .iter()
            .map(|gq| gq.quantity as f64 * y_i * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum::<f64>()
            + econ.labor_alloc.get(&factory.id).copied().unwrap_or(0.0) * econ.wage;
        profits.insert(factory.id.clone(), (revenue - cost).max(0.0));
    }

    let total_profit: f64 = profits.values().sum::<f64>().max(0.0);

    // Infrastructure imputed return from crowding
    let infra_return = global.crowding_sensitivity * crowding;

    // Split i_total between factories and infrastructure
    let total_return_signal = total_profit + infra_return;
    let (i_factories, i_infra) = if total_return_signal > 0.0 {
        (
            i_total * total_profit / total_return_signal,
            i_total * infra_return / total_return_signal,
        )
    } else {
        (i_total * 0.5, i_total * 0.5)
    };

    // Per-factory investment demand breakdown (needed for capital accumulation after rationing)
    let mut d_i: HashMap<String, f64> = HashMap::new();
    // factory_id -> Vec<(good, desired_qty)>
    let mut factory_investment_needs: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    // factory_id -> i_factory budget
    let mut factory_i_budget: HashMap<String, f64> = HashMap::new();

    let total_profit_denom = total_profit.max(1.0);
    for factory in &global.factory_types {
        let profit_share = profits.get(&factory.id).copied().unwrap_or(0.0) / total_profit_denom;
        let i_factory = i_factories * profit_share;
        factory_i_budget.insert(factory.id.clone(), i_factory);

        if !factory.build_cost.is_empty() {
            let total_build_cost: f64 = factory
                .build_cost
                .iter()
                .map(|gq| gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
                .sum::<f64>()
                .max(1.0);

            let mut needed: Vec<(String, f64)> = Vec::new();
            for gq in &factory.build_cost {
                let good_cost_share = gq.quantity as f64
                    * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                    / total_build_cost;
                let i_g = good_cost_share * i_factory
                    / econ.prices.get(&gq.good).copied().unwrap_or(1.0).max(0.01);
                *d_i.entry(gq.good.clone()).or_default() += i_g;
                needed.push((gq.good.clone(), i_g));
            }
            factory_investment_needs.insert(factory.id.clone(), needed);
        }
    }

    // Infrastructure investment demand (add to d_i)
    let mut infra_investment_needs: Vec<(String, f64)> = Vec::new();
    if let Some(icfg) = infra_cfg {
        if !icfg.build_cost.is_empty() {
            let total_infra_cost: f64 = icfg
                .build_cost
                .iter()
                .map(|gq| gq.quantity as f64 * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
                .sum::<f64>()
                .max(1.0);

            for gq in &icfg.build_cost {
                let good_cost_share = gq.quantity as f64
                    * econ.prices.get(&gq.good).copied().unwrap_or(1.0)
                    / total_infra_cost;
                let i_g = good_cost_share * i_infra
                    / econ.prices.get(&gq.good).copied().unwrap_or(1.0).max(0.01);
                *d_i.entry(gq.good.clone()).or_default() += i_g;
                infra_investment_needs.push((gq.good.clone(), i_g));
            }
        }
    }

    // D^X: export demand
    let exports = std::mem::take(&mut econ.exports_this_tick);
    let d_x: HashMap<String, f64> = exports;

    // Build aggregate demand categories
    let all_goods: std::collections::HashSet<String> = d_c
        .keys()
        .chain(d_i.keys())
        .chain(d_x.keys())
        .chain(d_m.keys())
        .chain(available_supply.keys())
        .chain(production_per_good.keys())
        .cloned()
        .collect();

    // D_dom = D^C + D^I + D^M, D_ration = D_dom + D^X, D_price = D_ration (intermediates now inside)
    let mut d_dom: HashMap<String, f64> = HashMap::new();
    let mut d_ration: HashMap<String, f64> = HashMap::new();
    let mut d_price: HashMap<String, f64> = HashMap::new();

    for good in &all_goods {
        let c = d_c.get(good).copied().unwrap_or(0.0);
        let i = d_i.get(good).copied().unwrap_or(0.0);
        let x = d_x.get(good).copied().unwrap_or(0.0);
        let m = d_m.get(good).copied().unwrap_or(0.0);

        d_dom.insert(good.clone(), c + i + m);
        d_ration.insert(good.clone(), c + i + m + x);
        d_price.insert(good.clone(), c + i + m + x);
    }

    // --- 7. Rationing with Domestic Priority ---
    let mut v_c: HashMap<String, f64> = HashMap::new();
    let mut v_i: HashMap<String, f64> = HashMap::new();
    let mut v_m: HashMap<String, f64> = HashMap::new();
    let mut v_x: HashMap<String, f64> = HashMap::new();

    for good in &all_goods {
        let o_g = available_supply.get(good).copied().unwrap_or(0.0);
        let dom = d_dom.get(good).copied().unwrap_or(0.0);
        let c = d_c.get(good).copied().unwrap_or(0.0);
        let i = d_i.get(good).copied().unwrap_or(0.0);
        let m = d_m.get(good).copied().unwrap_or(0.0);
        let x = d_x.get(good).copied().unwrap_or(0.0);

        if o_g >= dom {
            // Domestic fully satisfied; exports get remainder
            v_c.insert(good.clone(), c);
            v_i.insert(good.clone(), i);
            v_m.insert(good.clone(), m);
            v_x.insert(good.clone(), x.min(o_g - dom));
        } else if dom > 0.0 {
            // Domestic rationed proportionally; no exports
            let ratio = o_g / dom;
            v_c.insert(good.clone(), c * ratio);
            v_i.insert(good.clone(), i * ratio);
            v_m.insert(good.clone(), m * ratio);
            v_x.insert(good.clone(), 0.0);
        } else {
            v_c.insert(good.clone(), 0.0);
            v_i.insert(good.clone(), 0.0);
            v_m.insert(good.clone(), 0.0);
            v_x.insert(good.clone(), 0.0);
        }
    }

    // Set export fulfillment budget
    econ.last_exports_fulfilled = v_x.clone();

    // --- 7b. Scale production by intermediate fulfillment ---
    let mut inter_fulfill: HashMap<String, f64> = HashMap::new();
    for (good, &dm) in &d_m {
        let vm = v_m.get(good).copied().unwrap_or(0.0);
        inter_fulfill.insert(good.clone(), if dm > 0.0 { (vm / dm).min(1.0) } else { 1.0 });
    }

    // Recompute factory_runs, production, pending based on intermediate fulfillment
    for factory in &global.factory_types {
        if factory.inputs.is_empty() { continue; }
        let scale = factory.inputs.iter()
            .map(|gq| inter_fulfill.get(&gq.good).copied().unwrap_or(1.0))
            .fold(1.0_f64, f64::min);
        if scale < 1.0 {
            let old_y = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
            let new_y = old_y * scale;
            factory_runs.insert(factory.id.clone(), new_y);
            // Adjust production_per_good and pending_production
            for output in &factory.outputs {
                let delta = output.quantity as f64 * (old_y - new_y) * dt;
                *production_per_good.entry(output.good.clone()).or_default() -= delta;
                *pending_production.entry(output.good.clone()).or_default() -= delta;
            }
        }
    }

    // Recompute actual intermediates consumed from scaled factory_runs
    let mut actual_m: HashMap<String, f64> = HashMap::new();
    for factory in &global.factory_types {
        let y_i = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
        for input in &factory.inputs {
            *actual_m.entry(input.good.clone()).or_default() += input.quantity as f64 * y_i * dt;
        }
    }

    // --- 8. Stock Update ---
    // Compute total realized consumption per good (used here and in step 12 for revenue)
    let mut total_v: HashMap<String, f64> = HashMap::new();
    for good in &all_goods {
        let tv = v_c.get(good).copied().unwrap_or(0.0)
            + v_i.get(good).copied().unwrap_or(0.0)
            + actual_m.get(good).copied().unwrap_or(0.0)
            + v_x.get(good).copied().unwrap_or(0.0);
        total_v.insert(good.clone(), tv);
    }

    // Deduct realized usage
    for good in &all_goods {
        let tv = total_v.get(good).copied().unwrap_or(0.0);
        if tv > 0.0 {
            let stock = econ.stocks.entry(good.clone()).or_insert(0.0);
            *stock = (*stock - tv).max(0.0);
        }
    }

    // Credit THIS tick's production (one-tick delay: can't be consumed until next tick)
    for (good, qty) in &pending_production {
        *econ.stocks.entry(good.clone()).or_insert(0.0) += qty;
    }

    // --- 8b. Marginal cost per output good (for price floor) ---
    let mut marginal_costs: HashMap<String, f64> = HashMap::new();
    let mut mc_output_totals: HashMap<String, f64> = HashMap::new();

    for factory in &global.factory_types {
        let y_i = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
        if y_i <= 0.0 { continue; }

        let l_i = econ.labor_alloc.get(&factory.id).copied().unwrap_or(0.0);
        let labor_cost = econ.wage * l_i;
        let input_cost: f64 = factory.inputs.iter()
            .map(|gq| gq.quantity as f64 * y_i * econ.prices.get(&gq.good).copied().unwrap_or(1.0))
            .sum();
        let total_cost = labor_cost + input_cost;

        let total_output_qty: f64 = factory.outputs.iter()
            .map(|gq| gq.quantity as f64 * y_i)
            .sum();
        if total_output_qty <= 0.0 { continue; }

        for output in &factory.outputs {
            let qty = output.quantity as f64 * y_i;
            let cost_for_good = total_cost * qty / total_output_qty;
            *marginal_costs.entry(output.good.clone()).or_default() += cost_for_good;
            *mc_output_totals.entry(output.good.clone()).or_default() += qty;
        }
    }

    for (good, total_qty) in &mc_output_totals {
        if *total_qty > 0.0 {
            if let Some(mc) = marginal_costs.get_mut(good) {
                *mc /= total_qty;
            }
        }
    }

    // --- 9. Price Formation (D_price includes intermediate, O_g is stock-inclusive) ---
    for good in &all_goods {
        // Transient goods have zero carry-over stock so the stock-based supply
        // signal is always "maximum excess demand".  Use this tick's production
        // as the supply signal instead — it reflects actual capacity.
        if global.transient_goods.contains(good) {
            let d_g = d_price.get(good).copied().unwrap_or(0.0);
            let o_g = production_per_good.get(good).copied().unwrap_or(0.0);
            let denom = (d_g + o_g).max(1.0);
            let signal = (d_g - o_g) / denom;

            let prev_signal = econ.last_price_signal.get(good).copied().unwrap_or(0.0);
            let smoothed = global.eta * signal + (1.0 - global.eta) * prev_signal;
            econ.last_price_signal.insert(good.clone(), smoothed);

            let price = econ.prices.entry(good.clone()).or_insert(1.0);
            *price *= 1.0 + (global.mu * smoothed + global.cost_push_nu * wage_change) * dt;
            let floor = marginal_costs.get(good).copied().unwrap_or(0.01).clamp(0.01, 100_000.0);
            *price = price.clamp(floor, 100_000.0);
            continue;
        }

        let d_g = d_price.get(good).copied().unwrap_or(0.0);
        let o_g = available_supply.get(good).copied().unwrap_or(0.0);
        let denom = (d_g + o_g).max(1.0);
        let signal = (d_g - o_g) / denom;

        let prev_signal = econ.last_price_signal.get(good).copied().unwrap_or(0.0);
        let smoothed = global.eta * signal + (1.0 - global.eta) * prev_signal;
        econ.last_price_signal.insert(good.clone(), smoothed);

        let price = econ.prices.entry(good.clone()).or_insert(1.0);
        *price *= 1.0 + (global.mu * smoothed + global.cost_push_nu * wage_change) * dt;
        let floor = marginal_costs.get(good).copied().unwrap_or(0.01).clamp(0.01, 100_000.0);
        *price = price.clamp(floor, 100_000.0);
    }

    // --- 10. Factory Capital Accumulation (uses rationed V^I, i_factories budget) ---
    let mut investment_per_good: HashMap<String, f64> = HashMap::new();
    let mut total_actual_factory_investment = 0.0_f64;

    for factory in &global.factory_types {
        let i_factory = factory_i_budget.get(&factory.id).copied().unwrap_or(0.0);

        // Compute per-factory allocation of V^I
        let needs = factory_investment_needs
            .get(&factory.id)
            .cloned()
            .unwrap_or_default();

        let fulfillment = needs
            .iter()
            .map(|(good, qty)| {
                if *qty > 0.0 {
                    let total_d_i_g = d_i.get(good).copied().unwrap_or(0.0);
                    let v_i_g = v_i.get(good).copied().unwrap_or(0.0);
                    let allocated = if total_d_i_g > 0.0 {
                        v_i_g * (qty / total_d_i_g)
                    } else {
                        0.0
                    };
                    (allocated / qty).min(1.0)
                } else {
                    1.0
                }
            })
            .fold(1.0_f64, f64::min);

        // Record actual investment per good
        for (good, qty) in &needs {
            let total_d_i_g = d_i.get(good).copied().unwrap_or(0.0);
            let v_i_g = v_i.get(good).copied().unwrap_or(0.0);
            let allocated = if total_d_i_g > 0.0 {
                v_i_g * (qty / total_d_i_g)
            } else {
                0.0
            };
            let consumed = allocated.min(*qty * fulfillment);
            *investment_per_good.entry(good.clone()).or_default() += consumed;
        }

        let k = econ.capital.entry(factory.id.clone()).or_insert(0.0);
        let actual_invested = i_factory * fulfillment;
        total_actual_factory_investment += actual_invested;

        let va_i = va_per_factory.get(&factory.id).copied().unwrap_or(0.0);

        *k = (1.0 - global.depreciation * dt) * (*k) + global.capital_efficiency * actual_invested;

        if va_i > 0.0 {
            *k = k.max(0.01);
        } else {
            *k = k.max(0.0);
        }
    }

    // --- 11. Infrastructure Accumulation ---
    let mut infra_investment_per_good: HashMap<String, f64> = HashMap::new();
    let actual_infra_invested;
    {
        let infra_fulfillment = if infra_investment_needs.is_empty() {
            // No build cost defined — full fulfillment (monetary only)
            1.0
        } else {
            infra_investment_needs
                .iter()
                .map(|(good, qty)| {
                    if *qty > 0.0 {
                        let total_d_i_g = d_i.get(good).copied().unwrap_or(0.0);
                        let v_i_g = v_i.get(good).copied().unwrap_or(0.0);
                        let allocated = if total_d_i_g > 0.0 {
                            v_i_g * (qty / total_d_i_g)
                        } else {
                            0.0
                        };
                        (allocated / qty).min(1.0)
                    } else {
                        1.0
                    }
                })
                .fold(1.0_f64, f64::min)
        };

        // Record infra investment per good
        for (good, qty) in &infra_investment_needs {
            let total_d_i_g = d_i.get(good).copied().unwrap_or(0.0);
            let v_i_g = v_i.get(good).copied().unwrap_or(0.0);
            let allocated = if total_d_i_g > 0.0 {
                v_i_g * (qty / total_d_i_g)
            } else {
                0.0
            };
            let consumed = allocated.min(*qty * infra_fulfillment);
            *investment_per_good.entry(good.clone()).or_default() += consumed;
            *infra_investment_per_good.entry(good.clone()).or_default() += consumed;
        }

        actual_infra_invested = i_infra * infra_fulfillment;
        econ.infrastructure = (1.0 - global.infra_depreciation * dt) * econ.infrastructure
            + global.infra_capital_efficiency * actual_infra_invested;
        econ.infrastructure = econ.infrastructure.max(0.0);
    }

    // --- 11.5. Wealth Accumulation ---
    {
        let total_actual_investment = total_actual_factory_investment + actual_infra_invested;
        let unspent = (i_total - total_actual_investment).max(0.0);
        econ.wealth += unspent - global.mpc_wealth * econ.wealth;
        econ.wealth = econ.wealth.max(0.0);
    }

    // --- 12. Income = GDP = value added (revenue - intermediate costs) ---
    // Workers and capitalists can only split what the economy actually produces.
    let total_revenue: f64 = all_goods.iter()
        .map(|g| {
            total_v.get(g).copied().unwrap_or(0.0)
                * econ.prices.get(g).copied().unwrap_or(1.0)
        })
        .sum();
    let intermediate_cost: f64 = actual_m.iter()
        .map(|(g, &qty)| qty * econ.prices.get(g).copied().unwrap_or(1.0))
        .sum();
    econ.national_income = (total_revenue - intermediate_cost).max(0.0);

    // --- 13. Savings ---
    let income_change = if prev_income > 0.0 {
        (econ.national_income - prev_income) / prev_income
    } else {
        0.0
    };
    econ.savings_rate = (global.s_0
        + global.chi * income_change
        + global.psi_savings * (u_star - econ.unemployment))
        .clamp(0.05, 0.60);

    // --- 14. Demographics (moved here — births nerfed by consumption fulfillment) ---
    {
        // Consumption fulfillment ratio (uses d_c and v_c from THIS tick)
        let fulfillment_ratio = if d_c.is_empty() {
            1.0
        } else {
            let sum: f64 = d_c
                .iter()
                .map(|(g, &desired)| {
                    if desired > 0.0 {
                        (v_c.get(g).copied().unwrap_or(0.0) / desired).min(1.0)
                    } else {
                        1.0
                    }
                })
                .sum::<f64>();
            (sum / d_c.len() as f64).clamp(0.0, 1.0)
        };

        let deprivation = 1.0 + global.deprivation_mortality * (1.0 - fulfillment_ratio);

        let births = planet.fertility_base
            * econ.demographics.pop_active
            * (1.0 - 0.3 * crowding).max(0.1)
            * fulfillment_ratio
            * dt;
        let deaths_young = planet.mortality_base
            * global.mortality_young_mult
            * econ.demographics.pop_young
            * deprivation
            * dt;
        let deaths_active = planet.mortality_base
            * global.mortality_active_mult
            * econ.demographics.pop_active
            * deprivation
            * dt;
        let deaths_old = planet.mortality_base
            * global.mortality_old_mult
            * econ.demographics.pop_old
            * deprivation
            * dt;

        let age_young_to_active = global.aging_young_to_active * econ.demographics.pop_young * dt;
        let age_active_to_old = global.aging_active_to_old * econ.demographics.pop_active * dt;

        econ.demographics.pop_young += births - deaths_young - age_young_to_active;
        econ.demographics.pop_active += age_young_to_active - deaths_active - age_active_to_old;
        econ.demographics.pop_old += age_active_to_old - deaths_old;

        econ.demographics.pop_young = econ.demographics.pop_young.max(0.0);
        econ.demographics.pop_active = econ.demographics.pop_active.max(0.0);
        econ.demographics.pop_old = econ.demographics.pop_old.max(0.0);
    }

    // --- 15. Resources — deduct extraction + regenerate (dt-scaled) ---
    for factory in &global.factory_types {
        if !factory.is_extraction() {
            continue;
        }
        let y_i = factory_runs.get(&factory.id).copied().unwrap_or(0.0);
        for output in &factory.outputs {
            let consumed = output.quantity as f64 * y_i * dt;
            if let Some(stock) = econ.resources.get_mut(&output.good) {
                *stock = (*stock - consumed).max(0.0);
                if let Some(pr) = planet_resources.get(&output.good) {
                    if pr.renewable {
                        let regen =
                            pr.regeneration_rate * (1.0 - *stock / pr.max_capacity.max(1.0)) * dt;
                        *stock = (*stock + regen).min(pr.max_capacity);
                    }
                }
            }
        }
    }

    // --- 16. Store snapshots ---
    econ.last_production = production_per_good;
    econ.last_demand_c = d_c;
    econ.last_consumption = v_c;
    econ.last_investment = investment_per_good;
    econ.last_infra_investment = infra_investment_per_good;
    econ.last_intermediate = actual_m;
    econ.last_available_supply = available_supply;
    econ.last_demand = d_price;
    econ.last_crowding = crowding;
    econ.last_carrying_capacity = carrying_cap;

    econ.prev_wage = econ.wage;

    // --- 17. Clear transient goods from stocks ---
    for good_id in &global.transient_goods {
        econ.stocks.remove(good_id);
    }
}
