use serde::{Deserialize, Serialize};

use super::{Settlement, SpaceElevator, Station};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GasGiantType {
    Jovian,
    Saturnian,
    IceGiant,
    HotJupiter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum PlanetType {
    Telluric { climate: ClimateType },
    GasGiant { gas_type: GasGiantType },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    pub id: String,
    pub name: String,
    pub position: u32,
    pub distance_ua: f64,
    pub planet_type: PlanetType,
    #[serde(flatten)]
    pub status: PlanetStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanetRequest {
    pub name: String,
    pub position: u32,
    pub distance_ua: f64,
    pub planet_type: PlanetType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlanetRequest {
    pub name: Option<String>,
    pub distance_ua: Option<f64>,
    pub planet_type: Option<PlanetType>,
}
