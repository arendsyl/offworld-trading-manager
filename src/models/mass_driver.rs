use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassDriver {
    pub max_channels: u32,
}

impl MassDriver {
    pub fn new(max_channels: u32) -> Self {
        Self { max_channels }
    }

    pub fn active_connections_count(&self, connections: &[&MassDriverConnection]) -> u32 {
        connections
            .iter()
            .filter(|c| matches!(c.status, ConnectionStatus::Active))
            .count() as u32
    }

    pub fn has_available_channel(&self, connections: &[&MassDriverConnection]) -> bool {
        self.active_connections_count(connections) < self.max_channels
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Pending,
    Active,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassDriverConnection {
    pub id: Uuid,
    pub system: String,
    pub from_planet: String,
    pub to_planet: String,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConnectionRequest {
    pub system: String,
    pub from_planet: String,
    pub to_planet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAction {
    Accept,
    Reject,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConnectionRequest {
    pub action: ConnectionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketItem {
    pub good_name: String,
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SendMessage {
    Packet {
        connection_id: Uuid,
        items: Vec<PacketItem>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotifyMessage {
    ConnectionRequest {
        connection_id: Uuid,
        from_planet: String,
    },
    ConnectionAccepted {
        connection_id: Uuid,
    },
    ConnectionRejected {
        connection_id: Uuid,
    },
    ConnectionClosed {
        connection_id: Uuid,
        closed_by: String,
    },
    PacketReceived {
        connection_id: Uuid,
        from_planet: String,
        items: Vec<PacketItem>,
    },
    PacketSent {
        connection_id: Uuid,
        items: Vec<PacketItem>,
    },
    PacketRejected {
        connection_id: Uuid,
        reason: String,
    },
}
