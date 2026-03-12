use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    InstallStation,
    FoundSettlement,
    UpgradeDockingBays,
    UpgradeMassDriverChannels,
    UpgradeStorage,
    UpgradeElevatorCabins,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    InTransit,
    Building,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[serde(default)]
    pub transit_ends_at: Option<u64>,
    #[serde(default)]
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "project_type", rename_all = "snake_case")]
pub enum CreateProjectRequest {
    InstallStation {
        source_planet_id: String,
        target_planet_id: String,
        station_name: String,
    },
    FoundSettlement {
        source_planet_id: String,
        target_planet_id: String,
        settlement_name: String,
        station_name: String,
        #[serde(default)]
        extra_goods: HashMap<String, u64>,
    },
    UpgradeDockingBays {
        planet_id: String,
    },
    UpgradeMassDriverChannels {
        planet_id: String,
    },
    UpgradeStorage {
        planet_id: String,
    },
    UpgradeElevatorCabins {
        planet_id: String,
    },
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
