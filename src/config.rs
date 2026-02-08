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

fn default_admin_token() -> String {
    "admin-secret-token".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_token")]
    pub token: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            token: default_admin_token(),
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
    pub market: MarketConfig,
    #[serde(default)]
    pub admin: AdminConfig,
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
            market: MarketConfig::default(),
            admin: AdminConfig::default(),
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

    config
}
