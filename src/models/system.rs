use serde::{Deserialize, Serialize};

use super::Planet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarType {
    RedDwarf,
    YellowDwarf,
    BlueGiant,
    RedGiant,
    WhiteDwarf,
    Neutron,
    BinarySystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinates {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System {
    pub name: String,
    pub coordinates: Coordinates,
    pub star_type: StarType,
    pub planets: Vec<Planet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSystemRequest {
    pub name: String,
    pub coordinates: Coordinates,
    pub star_type: StarType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSystemRequest {
    pub name: Option<String>,
    pub coordinates: Option<Coordinates>,
    pub star_type: Option<StarType>,
}
