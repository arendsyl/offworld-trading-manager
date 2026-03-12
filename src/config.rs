use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulsarConfig {
    #[serde(default = "default_pulsar_url")]
    pub url: String,
    #[serde(default = "default_tenant")]
    pub tenant: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

fn default_pulsar_url() -> String {
    "pulsar://localhost:6650".to_string()
}

fn default_tenant() -> String {
    "public".to_string()
}

fn default_namespace() -> String {
    "default".to_string()
}

impl Default for PulsarConfig {
    fn default() -> Self {
        Self {
            url: default_pulsar_url(),
            tenant: default_tenant(),
            namespace: default_namespace(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassDriverDefaults {
    #[serde(default = "default_channels")]
    pub default_channels: u32,
    #[serde(default = "default_max_packet_size")]
    pub max_packet_size: u64,
    #[serde(default = "default_au_to_seconds")]
    pub au_to_seconds: f64,
}

fn default_channels() -> u32 {
    4
}

fn default_max_packet_size() -> u64 {
    20
}

fn default_au_to_seconds() -> f64 {
    2.0
}

impl Default for MassDriverDefaults {
    fn default() -> Self {
        Self {
            default_channels: default_channels(),
            max_packet_size: default_max_packet_size(),
            au_to_seconds: default_au_to_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipConfig {
    #[serde(default = "default_ship_au_to_seconds")]
    pub au_to_seconds: f64,
    #[serde(default = "default_seconds_per_unit")]
    pub seconds_per_unit: f64,
    #[serde(default = "default_webhook_timeout_secs")]
    pub webhook_timeout_secs: u64,
}

fn default_ship_au_to_seconds() -> f64 {
    2.0
}

fn default_seconds_per_unit() -> f64 {
    0.1
}

fn default_webhook_timeout_secs() -> u64 {
    5
}

impl Default for ShipConfig {
    fn default() -> Self {
        Self {
            au_to_seconds: default_ship_au_to_seconds(),
            seconds_per_unit: default_seconds_per_unit(),
            webhook_timeout_secs: default_webhook_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConfig {
    #[serde(default = "default_trade_channel_capacity")]
    pub trade_channel_capacity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruckingConfig {
    #[serde(default = "default_trucking_jump_range_ly")]
    pub jump_range_ly: f64,
    #[serde(default = "default_trucking_jump_time_secs")]
    pub jump_time_secs: f64,
    #[serde(default = "default_trucking_au_to_seconds")]
    pub au_to_seconds: f64,
    #[serde(default = "default_trucking_seconds_per_unit")]
    pub seconds_per_unit: f64,
    #[serde(default = "default_trucking_base_fee")]
    pub base_fee: u64,
    #[serde(default = "default_trucking_fee_per_unit")]
    pub fee_per_unit: f64,
}

fn default_trucking_jump_range_ly() -> f64 {
    5.0
}

fn default_trucking_jump_time_secs() -> f64 {
    3.0
}

fn default_trucking_au_to_seconds() -> f64 {
    2.0
}

fn default_trucking_seconds_per_unit() -> f64 {
    0.1
}

fn default_trucking_base_fee() -> u64 {
    100
}

fn default_trucking_fee_per_unit() -> f64 {
    1.0
}

impl Default for TruckingConfig {
    fn default() -> Self {
        Self {
            jump_range_ly: default_trucking_jump_range_ly(),
            jump_time_secs: default_trucking_jump_time_secs(),
            au_to_seconds: default_trucking_au_to_seconds(),
            seconds_per_unit: default_trucking_seconds_per_unit(),
            base_fee: default_trucking_base_fee(),
            fee_per_unit: default_trucking_fee_per_unit(),
        }
    }
}

fn default_trade_tick_duration_secs() -> f64 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeConfig {
    #[serde(default = "default_trade_tick_duration_secs")]
    pub tick_duration_secs: f64,
}

impl Default for TradeConfig {
    fn default() -> Self {
        Self {
            tick_duration_secs: default_trade_tick_duration_secs(),
        }
    }
}

fn default_admin_token() -> String {
    "admin-secret-token".to_string()
}

fn default_biscuit_private_key_hex() -> String {
    // Dev-only default key — override via config or BISCUIT_PRIVATE_KEY env var in production
    "a0907d384d28b8fa73840c618b37e04af3ef22d1f181b59a8b15d36df6c6460a".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_token")]
    pub token: String,
    #[serde(default = "default_biscuit_private_key_hex")]
    pub biscuit_private_key_hex: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            token: default_admin_token(),
            biscuit_private_key_hex: default_biscuit_private_key_hex(),
        }
    }
}

fn default_trade_channel_capacity() -> usize {
    1024
}

impl Default for MarketConfig {
    fn default() -> Self {
        Self {
            trade_channel_capacity: default_trade_channel_capacity(),
        }
    }
}

fn default_station_install_fee() -> u64 { 5000 }
fn default_station_install_goods() -> HashMap<String, u64> {
    [("steel".into(), 100), ("electronics".into(), 50)].into()
}
fn default_settlement_found_fee() -> u64 { 10000 }
fn default_settlement_found_goods() -> HashMap<String, u64> {
    [("steel".into(), 200), ("electronics".into(), 100), ("food".into(), 50)].into()
}
fn default_upgrade_docking_bay_fee() -> u64 { 1000 }
fn default_upgrade_docking_bay_goods() -> HashMap<String, u64> { HashMap::new() }
fn default_upgrade_mass_driver_fee() -> u64 { 1500 }
fn default_upgrade_mass_driver_goods() -> HashMap<String, u64> { HashMap::new() }
fn default_upgrade_storage_fee() -> u64 { 500 }
fn default_upgrade_storage_goods() -> HashMap<String, u64> { HashMap::new() }
fn default_storage_increment() -> u64 { 500 }
fn default_upgrade_cabin_fee() -> u64 { 2000 }
fn default_upgrade_cabin_goods() -> HashMap<String, u64> { HashMap::new() }
fn default_build_base_secs() -> f64 { 30.0 }
fn default_upgrade_build_secs() -> f64 { 10.0 }
fn default_initial_docking_bays() -> u32 { 2 }
fn default_initial_max_storage() -> u64 { 10000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructionConfig {
    #[serde(default = "default_station_install_fee")]
    pub station_install_fee: u64,
    #[serde(default = "default_station_install_goods")]
    pub station_install_goods: HashMap<String, u64>,
    #[serde(default = "default_settlement_found_fee")]
    pub settlement_found_fee: u64,
    #[serde(default = "default_settlement_found_goods")]
    pub settlement_found_goods: HashMap<String, u64>,
    #[serde(default = "default_upgrade_docking_bay_fee")]
    pub upgrade_docking_bay_fee: u64,
    #[serde(default = "default_upgrade_docking_bay_goods")]
    pub upgrade_docking_bay_goods: HashMap<String, u64>,
    #[serde(default = "default_upgrade_mass_driver_fee")]
    pub upgrade_mass_driver_fee: u64,
    #[serde(default = "default_upgrade_mass_driver_goods")]
    pub upgrade_mass_driver_goods: HashMap<String, u64>,
    #[serde(default = "default_upgrade_storage_fee")]
    pub upgrade_storage_fee: u64,
    #[serde(default = "default_upgrade_storage_goods")]
    pub upgrade_storage_goods: HashMap<String, u64>,
    #[serde(default = "default_storage_increment")]
    pub storage_increment: u64,
    #[serde(default = "default_upgrade_cabin_fee")]
    pub upgrade_cabin_fee: u64,
    #[serde(default = "default_upgrade_cabin_goods")]
    pub upgrade_cabin_goods: HashMap<String, u64>,
    #[serde(default = "default_build_base_secs")]
    pub build_base_secs: f64,
    #[serde(default = "default_upgrade_build_secs")]
    pub upgrade_build_secs: f64,
    #[serde(default = "default_initial_docking_bays")]
    pub initial_docking_bays: u32,
    #[serde(default = "default_initial_max_storage")]
    pub initial_max_storage: u64,
}

impl Default for ConstructionConfig {
    fn default() -> Self {
        Self {
            station_install_fee: default_station_install_fee(),
            station_install_goods: default_station_install_goods(),
            settlement_found_fee: default_settlement_found_fee(),
            settlement_found_goods: default_settlement_found_goods(),
            upgrade_docking_bay_fee: default_upgrade_docking_bay_fee(),
            upgrade_docking_bay_goods: default_upgrade_docking_bay_goods(),
            upgrade_mass_driver_fee: default_upgrade_mass_driver_fee(),
            upgrade_mass_driver_goods: default_upgrade_mass_driver_goods(),
            upgrade_storage_fee: default_upgrade_storage_fee(),
            upgrade_storage_goods: default_upgrade_storage_goods(),
            storage_increment: default_storage_increment(),
            upgrade_cabin_fee: default_upgrade_cabin_fee(),
            upgrade_cabin_goods: default_upgrade_cabin_goods(),
            build_base_secs: default_build_base_secs(),
            upgrade_build_secs: default_upgrade_build_secs(),
            initial_docking_bays: default_initial_docking_bays(),
            initial_max_storage: default_initial_max_storage(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub auto_save_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub verbose: u8,
    pub seed: Option<String>,
    #[serde(default)]
    pub pulsar: PulsarConfig,
    #[serde(default)]
    pub mass_driver: MassDriverDefaults,
    #[serde(default)]
    pub ship: ShipConfig,
    #[serde(default)]
    pub trucking: TruckingConfig,
    #[serde(default)]
    pub market: MarketConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub construction: ConstructionConfig,
    #[serde(default)]
    pub trade: TradeConfig,
    #[serde(default)]
    pub economy: crate::economy::GlobalEconomyConfig,
    #[serde(default)]
    pub s3: S3Config,
    #[serde(default)]
    pub save_name: Option<String>,
}

fn default_port() -> u16 {
    3000
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            verbose: 0,
            seed: None,
            pulsar: PulsarConfig::default(),
            mass_driver: MassDriverDefaults::default(),
            ship: ShipConfig::default(),
            trucking: TruckingConfig::default(),
            market: MarketConfig::default(),
            admin: AdminConfig::default(),
            construction: ConstructionConfig::default(),
            trade: TradeConfig::default(),
            economy: crate::economy::GlobalEconomyConfig::default(),
            s3: S3Config::default(),
            save_name: None,
        }
    }
}

pub fn load_config(
    config_path: Option<&str>,
    cli_port: Option<u16>,
    cli_verbose: u8,
    cli_seed: Option<&str>,
) -> AppConfig {
    let mut config = if let Some(path) = config_path {
        let content = std::fs::read_to_string(Path::new(path))
            .unwrap_or_else(|e| panic!("Failed to read config file {}: {}", path, e));
        toml::from_str::<AppConfig>(&content)
            .unwrap_or_else(|e| panic!("Failed to parse config file {}: {}", path, e))
    } else {
        AppConfig::default()
    };

    // ENV overrides
    if let Ok(port) = std::env::var("PORT") {
        if let Ok(p) = port.parse::<u16>() {
            config.port = p;
        }
    }
    if let Ok(url) = std::env::var("PULSAR_URL") {
        config.pulsar.url = url;
    }
    if let Ok(token) = std::env::var("ADMIN_TOKEN") {
        config.admin.token = token;
    }
    if let Ok(key) = std::env::var("BISCUIT_PRIVATE_KEY") {
        config.admin.biscuit_private_key_hex = key;
    }
    if let Ok(bucket) = std::env::var("S3_BUCKET") {
        config.s3.bucket = Some(bucket);
    }
    if let Ok(endpoint) = std::env::var("S3_ENDPOINT") {
        config.s3.endpoint = Some(endpoint);
    }
    if let Ok(region) = std::env::var("AWS_REGION") {
        config.s3.region = Some(region);
    }
    if let Ok(key) = std::env::var("AWS_ACCESS_KEY_ID") {
        config.s3.access_key_id = Some(key);
    }
    if let Ok(secret) = std::env::var("AWS_SECRET_ACCESS_KEY") {
        config.s3.secret_access_key = Some(secret);
    }

    // ENV overrides for economy data paths
    if let Ok(path) = std::env::var("FACTORY_TYPES_PATH") {
        config.economy.factory_types_path = path;
    }
    if let Ok(path) = std::env::var("CONSUMPTIONS_PATH") {
        config.economy.consumptions_path = path;
    }
    if let Ok(path) = std::env::var("GOODS_PATH") {
        config.economy.goods_path = path;
    }

    // CLI overrides
    if let Some(p) = cli_port {
        config.port = p;
    }
    if cli_verbose > 0 {
        config.verbose = cli_verbose;
    }
    if let Some(seed) = cli_seed {
        config.seed = Some(seed.to_string());
    }

    // Load economy JSON data files
    load_economy_data(&mut config.economy);

    config
}

fn load_economy_data(economy: &mut crate::economy::GlobalEconomyConfig) {
    use crate::economy::config::{FactoryTypeConfig, GoodConfig};

    // Load factory types from JSON if not already populated (e.g. by TOML inline)
    if economy.factory_types.is_empty() {
        let path = &economy.factory_types_path;
        match std::fs::read_to_string(path) {
            Ok(content) => {
                economy.factory_types = serde_json::from_str::<Vec<FactoryTypeConfig>>(&content)
                    .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));
            }
            Err(e) => {
                tracing::warn!("Could not read factory types from {}: {} — using empty list", path, e);
            }
        }
    }

    // Load consumption profiles from JSON if not already populated
    if economy.consumption_profiles.is_empty() {
        let path = &economy.consumptions_path;
        match std::fs::read_to_string(path) {
            Ok(content) => {
                economy.consumption_profiles = serde_json::from_str(&content)
                    .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));
            }
            Err(e) => {
                tracing::warn!("Could not read consumption profiles from {}: {} — using empty map", path, e);
            }
        }
    }

    // Load goods registry from JSON if not already populated
    if economy.goods.is_empty() {
        let path = &economy.goods_path;
        match std::fs::read_to_string(path) {
            Ok(content) => {
                economy.goods = serde_json::from_str::<Vec<GoodConfig>>(&content)
                    .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));
            }
            Err(e) => {
                tracing::warn!("Could not read goods from {}: {}", path, e);
            }
        }
    }

    // Build transient goods set from registry
    economy.transient_goods = economy
        .goods
        .iter()
        .filter(|g| g.transient)
        .map(|g| g.id.clone())
        .collect();
}
