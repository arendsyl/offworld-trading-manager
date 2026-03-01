use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use super::Planet;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Coordinates {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct System {
    pub name: String,
    pub coordinates: Coordinates,
    pub star_type: StarType,
    pub planets: Vec<Planet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateSystemRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    pub coordinates: Coordinates,
    pub star_type: StarType,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateSystemRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub coordinates: Option<Coordinates>,
    pub star_type: Option<StarType>,
}
