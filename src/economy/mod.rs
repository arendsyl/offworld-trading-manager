pub mod config;
pub mod lifecycle;
pub mod models;
pub mod tick;

pub use config::{GlobalEconomyConfig, GoodConfig, PlanetEconomyConfig};
pub use lifecycle::spawn_economy_loop;
pub use models::{Demographics, EconomyState};
