use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::economy::PlanetEconomyConfig;

use super::{Settlement, SpaceElevator, Station};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PlanetStatus {
    Uninhabited,
    Settled { settlement: Settlement },
    Connected {
        settlement: Settlement,
        station: Station,
        space_elevator: SpaceElevator,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClimateType {
    Arid,
    Tropical,
    Temperate,
    Arctic,
    Desert,
    Oceanic,
    Volcanic,
}

impl std::fmt::Display for ClimateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arid => write!(f, "arid"),
            Self::Tropical => write!(f, "tropical"),
            Self::Temperate => write!(f, "temperate"),
            Self::Arctic => write!(f, "arctic"),
            Self::Desert => write!(f, "desert"),
            Self::Oceanic => write!(f, "oceanic"),
            Self::Volcanic => write!(f, "volcanic"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GasGiantType {
    Jovian,
    Saturnian,
    IceGiant,
    HotJupiter,
}

impl std::fmt::Display for GasGiantType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jovian => write!(f, "jovian"),
            Self::Saturnian => write!(f, "saturnian"),
            Self::IceGiant => write!(f, "ice_giant"),
            Self::HotJupiter => write!(f, "hot_jupiter"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum PlanetType {
    Telluric { climate: ClimateType },
    GasGiant { gas_type: GasGiantType },
}

impl std::fmt::Display for PlanetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Telluric { climate } => climate.fmt(f),
            Self::GasGiant { gas_type } => gas_type.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct PlanetResource {
    pub max_capacity: f64,
    pub renewable: bool,
    pub regeneration_rate: f64,
    pub max_extraction: f64,
    /// Michaelis-Menten half-saturation constant
    pub k_half: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Planet {
    pub id: String,
    pub name: String,
    pub position: u32,
    pub distance_ua: f64,
    #[serde(default)]
    pub resources: HashMap<String, PlanetResource>,
    #[serde(default)]
    pub economy_config: PlanetEconomyConfig,
    pub planet_type: PlanetType,
    #[serde(flatten)]
    pub status: PlanetStatus,
}

impl Planet {
    /// Get a mutable reference to the settlement, if this planet has one.
    pub fn settlement_mut(&mut self) -> Option<&mut Settlement> {
        match &mut self.status {
            PlanetStatus::Settled { settlement } => Some(settlement),
            PlanetStatus::Connected { settlement, .. } => Some(settlement),
            PlanetStatus::Uninhabited => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreatePlanetRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    pub position: u32,
    #[validate(range(min = 0.01, max = 1000.0))]
    pub distance_ua: f64,
    pub planet_type: PlanetType,
    pub resources: Option<HashMap<String, PlanetResource>>,
    pub economy_config: Option<PlanetEconomyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdatePlanetRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub distance_ua: Option<f64>,
    pub planet_type: Option<PlanetType>,
    pub economy_config: Option<PlanetEconomyConfig>,
}
