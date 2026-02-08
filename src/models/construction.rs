use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    InstallStation,
    FoundSettlement,
    UpgradeDockingBays,
    UpgradeMassDriverChannels,
    UpgradeStorage,
    UpgradeElevatorCabins,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    InTransit,
    Building,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructionProject {
    pub id: Uuid,
    pub owner_id: String,
    pub project_type: ProjectType,
    pub source_planet_id: String,
    pub target_planet_id: String,
    pub fee: u64,
    pub goods_consumed: HashMap<String, u64>,
    pub extra_goods: HashMap<String, u64>,
    pub status: ProjectStatus,
    pub created_at: u64,
    pub completion_at: u64,
    pub station_name: Option<String>,
    pub settlement_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStationRequest {
    pub source_planet_id: String,
    pub target_planet_id: String,
    pub station_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundSettlementRequest {
    pub source_planet_id: String,
    pub target_planet_id: String,
    pub settlement_name: String,
    pub station_name: String,
    #[serde(default)]
    pub extra_goods: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StationUpgradeType {
    DockingBays,
    MassDriverChannels,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStationRequest {
    pub planet_id: String,
    pub upgrade_type: StationUpgradeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeElevatorRequest {
    pub planet_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstructionWebhookPayload {
    ConstructionComplete {
        project_id: Uuid,
        project_type: ProjectType,
        target_planet_id: String,
    },
}
