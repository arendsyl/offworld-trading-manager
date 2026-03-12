use std::collections::HashMap;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoodConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub transient: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClimateInfraConfig {
    pub build_cost: Vec<GoodQuantity>,
    #[serde(default = "default_people_per_infra_unit")]
    pub people_per_unit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoodQuantity {
    pub good: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FactoryCategory {
    Extraction,
    #[default]
    Manufacturing,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FactoryTypeConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub category: FactoryCategory,
    #[serde(default)]
    pub inputs: Vec<GoodQuantity>,
    pub outputs: Vec<GoodQuantity>,
    #[serde(default)]
    pub build_cost: Vec<GoodQuantity>,
    /// Ticks for a full production cycle; per-tick output = qty * Y / production_cycle
    #[serde(default = "default_production_cycle")]
    pub production_cycle: f64,
}

impl FactoryTypeConfig {
    pub fn is_extraction(&self) -> bool {
        self.category == FactoryCategory::Extraction
    }
}

/// Universal simulation parameters — stays in AppConfig (TOML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEconomyConfig {
    #[serde(default = "default_tick_duration_secs")]
    pub tick_duration_secs: f64,

    #[serde(default)]
    pub factory_types: Vec<FactoryTypeConfig>,

    /// Path to factories JSON data file
    #[serde(default = "default_factory_types_path")]
    pub factory_types_path: String,

    /// Path to consumptions JSON data file
    #[serde(default = "default_consumptions_path")]
    pub consumptions_path: String,

    /// Goods registry loaded from JSON
    #[serde(default)]
    pub goods: Vec<GoodConfig>,

    /// Path to goods JSON data file
    #[serde(default = "default_goods_path")]
    pub goods_path: String,

    /// Set of transient good IDs (computed from goods registry)
    #[serde(skip)]
    pub transient_goods: HashSet<String>,

    // Cobb-Douglas exponents (alpha + beta ≈ 1.0)
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default = "default_beta")]
    pub beta: f64,

    // Price adjustment speed
    #[serde(default = "default_mu")]
    pub mu: f64,

    // Savings parameters
    #[serde(default = "default_s_0")]
    pub s_0: f64,
    #[serde(default = "default_chi")]
    pub chi: f64,
    #[serde(default = "default_psi_savings")]
    pub psi_savings: f64,
    #[serde(default = "default_eta")]
    pub eta: f64,

    // Capital parameters
    #[serde(default = "default_depreciation")]
    pub depreciation: f64,
    #[serde(default = "default_capital_efficiency")]
    pub capital_efficiency: f64,

    // Phillips curve
    #[serde(default = "default_phillips_kappa")]
    pub phillips_kappa: f64,

    // Consumption profiles: climate/gas_type key -> good -> structural budget share (α_g^C)
    #[serde(default)]
    pub consumption_profiles: HashMap<String, HashMap<String, f64>>,

    #[serde(default = "default_substitution_elasticity")]
    pub substitution_elasticity: f64,

    /// Labor mobility friction: 0..1, where 1.0 = instant reallocation (backward compat)
    #[serde(default = "default_labor_mobility")]
    pub labor_mobility: f64,

    // Infrastructure parameters
    #[serde(default = "default_infra_depreciation")]
    pub infra_depreciation: f64,

    #[serde(default = "default_infra_capital_efficiency")]
    pub infra_capital_efficiency: f64,

    #[serde(default = "default_crowding_sensitivity")]
    pub crowding_sensitivity: f64,

    /// Per-climate infrastructure config (build cost + density)
    #[serde(default)]
    pub infra_climate: HashMap<String, ClimateInfraConfig>,

    // Demographics — aging transition rates (per unit time)
    #[serde(default = "default_aging_young_to_active")]
    pub aging_young_to_active: f64,
    #[serde(default = "default_aging_active_to_old")]
    pub aging_active_to_old: f64,

    // Demographics — mortality multipliers per cohort (applied to planet mortality_base)
    #[serde(default = "default_mortality_young_mult")]
    pub mortality_young_mult: f64,
    #[serde(default = "default_mortality_active_mult")]
    pub mortality_active_mult: f64,
    #[serde(default = "default_mortality_old_mult")]
    pub mortality_old_mult: f64,

    #[serde(default = "default_deprivation_mortality")]
    pub deprivation_mortality: f64,

    #[serde(default = "default_cost_push_nu")]
    pub cost_push_nu: f64,

    #[serde(default = "default_mpc_wealth")]
    pub mpc_wealth: f64,
}

impl Default for GlobalEconomyConfig {
    fn default() -> Self {
        Self {
            tick_duration_secs: default_tick_duration_secs(),
            factory_types: Vec::new(),
            factory_types_path: default_factory_types_path(),
            consumptions_path: default_consumptions_path(),
            goods: Vec::new(),
            goods_path: default_goods_path(),
            transient_goods: HashSet::new(),
            alpha: default_alpha(),
            beta: default_beta(),
            mu: default_mu(),
            s_0: default_s_0(),
            chi: default_chi(),
            psi_savings: default_psi_savings(),
            eta: default_eta(),
            depreciation: default_depreciation(),
            capital_efficiency: default_capital_efficiency(),
            phillips_kappa: default_phillips_kappa(),
            consumption_profiles: HashMap::new(),
            substitution_elasticity: default_substitution_elasticity(),
            labor_mobility: default_labor_mobility(),
            infra_depreciation: default_infra_depreciation(),
            infra_capital_efficiency: default_infra_capital_efficiency(),
            crowding_sensitivity: default_crowding_sensitivity(),
            infra_climate: HashMap::new(),
            aging_young_to_active: default_aging_young_to_active(),
            aging_active_to_old: default_aging_active_to_old(),
            mortality_young_mult: default_mortality_young_mult(),
            mortality_active_mult: default_mortality_active_mult(),
            mortality_old_mult: default_mortality_old_mult(),
            deprivation_mortality: default_deprivation_mortality(),
            cost_push_nu: default_cost_push_nu(),
            mpc_wealth: default_mpc_wealth(),
        }
    }
}

/// Per-planet economy parameters — lives on each Planet.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanetEconomyConfig {
    // Demographics — affected by local conditions
    #[serde(default = "default_fertility_base")]
    pub fertility_base: f64,
    #[serde(default = "default_mortality_base")]
    pub mortality_base: f64,
    #[serde(default = "default_unemployment_natural")]
    pub unemployment_natural: f64,

    // Initial values — used only at economy initialization
    #[serde(default = "default_initial_pop_active")]
    pub initial_pop_active: f64,
    #[serde(default)]
    pub initial_prices: HashMap<String, f64>,
    #[serde(default)]
    pub initial_capital: HashMap<String, f64>,
    #[serde(default = "default_initial_wage")]
    pub initial_wage: f64,

    /// Base carrying capacity before infrastructure contribution
    #[serde(default = "default_base_carrying_capacity")]
    pub base_carrying_capacity: f64,

    #[serde(default = "default_initial_infrastructure")]
    pub initial_infrastructure: f64,
}

impl Default for PlanetEconomyConfig {
    fn default() -> Self {
        Self {
            fertility_base: default_fertility_base(),
            mortality_base: default_mortality_base(),
            unemployment_natural: default_unemployment_natural(),
            initial_pop_active: default_initial_pop_active(),
            initial_prices: HashMap::new(),
            initial_capital: HashMap::new(),
            initial_wage: default_initial_wage(),
            base_carrying_capacity: default_base_carrying_capacity(),
            initial_infrastructure: default_initial_infrastructure(),
        }
    }
}

/// Build default initial prices from factory types — one entry per unique output good.
/// Build initial prices from supply-chain costs.
/// Factories are processed in tier order so input prices are known before outputs.
/// Tier-0 (extraction) outputs get a base price; higher tiers use cost-plus pricing.
pub fn build_default_initial_prices(factory_types: &[FactoryTypeConfig]) -> HashMap<String, f64> {
    let base_price = 1.0;
    let markup = 1.3; // covers labor + capital margin

    let mut sorted: Vec<&FactoryTypeConfig> = factory_types.iter().collect();
    sorted.sort_by_key(|f| f.tier);

    let mut prices = HashMap::new();
    for factory in &sorted {
        let total_output_qty: f64 = factory.outputs.iter().map(|gq| gq.quantity as f64).sum();
        if total_output_qty <= 0.0 {
            continue;
        }

        let cost_per_unit = if factory.inputs.is_empty() {
            base_price
        } else {
            let input_cost: f64 = factory
                .inputs
                .iter()
                .map(|gq| gq.quantity as f64 * prices.get(&gq.good).copied().unwrap_or(base_price))
                .sum();
            (input_cost * markup) / total_output_qty
        };

        for output in &factory.outputs {
            prices.entry(output.good.clone()).or_insert(cost_per_unit);
        }
    }
    prices
}

/// Build default initial capital — one entry per factory type.
pub fn build_default_initial_capital(factory_types: &[FactoryTypeConfig]) -> HashMap<String, f64> {
    let mut capital = HashMap::new();
    for factory in factory_types {
        let base = match factory.tier {
            0 => 50.0,
            1 => 30.0,
            2 => 20.0,
            3 => 10.0,
            _ => 5.0,
        };
        capital.insert(factory.id.clone(), base);
    }
    capital
}

// --- Default value functions ---

fn default_tick_duration_secs() -> f64 {
    1.0
}
pub fn default_production_cycle() -> f64 {
    1.0
}

fn default_factory_types_path() -> String {
    "data/factories.json".to_string()
}

fn default_consumptions_path() -> String {
    "data/consumptions.json".to_string()
}

fn default_goods_path() -> String {
    "data/goods.json".to_string()
}

fn default_alpha() -> f64 {
    0.40
}
fn default_beta() -> f64 {
    0.60
}
fn default_mu() -> f64 {
    0.005
}
fn default_s_0() -> f64 {
    0.22
}
fn default_chi() -> f64 {
    0.05
}
fn default_psi_savings() -> f64 {
    0.05
}
fn default_eta() -> f64 {
    0.25
}
fn default_depreciation() -> f64 {
    0.005
}
fn default_capital_efficiency() -> f64 {
    0.8
}
fn default_fertility_base() -> f64 {
    0.04
}
fn default_mortality_base() -> f64 {
    0.01
}
fn default_unemployment_natural() -> f64 {
    0.05
}
fn default_phillips_kappa() -> f64 {
    0.01
}
fn default_substitution_elasticity() -> f64 {
    -0.5
}

fn default_initial_pop_active() -> f64 {
    100.0
}

fn default_initial_wage() -> f64 {
    10.0
}

fn default_labor_mobility() -> f64 {
    0.3
}
fn default_base_carrying_capacity() -> f64 {
    100.0
}

fn default_infra_depreciation() -> f64 {
    0.003
}
fn default_infra_capital_efficiency() -> f64 {
    0.8
}
fn default_crowding_sensitivity() -> f64 {
    1.0
}
pub fn default_people_per_infra_unit() -> f64 {
    10.0
}
fn default_initial_infrastructure() -> f64 {
    10.0
}
fn default_aging_young_to_active() -> f64 {
    0.05
}
fn default_aging_active_to_old() -> f64 {
    0.01
}
fn default_mortality_young_mult() -> f64 {
    0.3
}
fn default_mortality_active_mult() -> f64 {
    0.5
}
fn default_mortality_old_mult() -> f64 {
    4.5
}
fn default_deprivation_mortality() -> f64 {
    2.0
}
fn default_cost_push_nu() -> f64 {
    0.10
}
fn default_mpc_wealth() -> f64 {
    0.03
}
