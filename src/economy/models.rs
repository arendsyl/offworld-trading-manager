use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct Demographics {
    #[serde(default)]
    pub pop_young: f64,
    #[serde(default)]
    pub pop_active: f64,
    #[serde(default)]
    pub pop_old: f64,
}

impl Demographics {
    pub fn total_population(&self) -> f64 {
        self.pop_young + self.pop_active + self.pop_old
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct EconomyState {
    #[serde(default)]
    pub demographics: Demographics,
    /// K_i per factory
    #[serde(default)]
    pub capital: HashMap<String, f64>,
    /// L_i (computed each tick)
    #[serde(default)]
    pub labor_alloc: HashMap<String, f64>,
    /// P_g — price per good
    #[serde(default)]
    pub prices: HashMap<String, f64>,
    #[serde(default)]
    pub wage: f64,
    #[serde(default)]
    pub prev_wage: f64,
    /// Accumulated financial wealth (unspent savings buffer)
    #[serde(default)]
    pub wealth: f64,
    #[serde(default)]
    pub unemployment: f64,
    #[serde(default)]
    pub national_income: f64,
    #[serde(default)]
    pub savings_rate: f64,
    #[serde(default)]
    pub resources: HashMap<String, f64>,

    /// Physical goods stockpile
    #[serde(default)]
    pub stocks: HashMap<String, f64>,

    // Trade flow accumulators — accumulated by trade_lifecycle between economy ticks
    #[serde(default)]
    pub imports_this_tick: HashMap<String, f64>,
    #[serde(default)]
    pub exports_this_tick: HashMap<String, f64>,

    // Last-tick flow snapshots (for API visibility, read-only)
    #[serde(default)]
    pub last_production: HashMap<String, f64>,
    #[serde(default)]
    pub last_consumption: HashMap<String, f64>,
    #[serde(default)]
    pub last_investment: HashMap<String, f64>,
    #[serde(default)]
    pub last_intermediate: HashMap<String, f64>,
    #[serde(default)]
    pub last_available_supply: HashMap<String, f64>,
    #[serde(default)]
    pub last_demand: HashMap<String, f64>,

    /// EMA-smoothed price signal per good (persists across ticks)
    #[serde(default)]
    pub last_price_signal: HashMap<String, f64>,

    /// Export fulfillment budget — set by economy tick, consumed by trade_lifecycle
    #[serde(default)]
    pub last_exports_fulfilled: HashMap<String, f64>,

    /// Infrastructure capital stock
    #[serde(default)]
    pub infrastructure: f64,

    /// Last-tick infrastructure investment per good
    #[serde(default)]
    pub last_infra_investment: HashMap<String, f64>,

    /// Last-tick crowding ratio
    #[serde(default)]
    pub last_crowding: f64,

    /// Last-tick carrying capacity
    #[serde(default)]
    pub last_carrying_capacity: f64,

    /// Last-tick desired consumption demand (for fulfillment ratio)
    #[serde(default)]
    pub last_demand_c: HashMap<String, f64>,

    /// Whether this economy has been initialized (false for seed defaults)
    #[serde(default)]
    pub initialized: bool,
}
