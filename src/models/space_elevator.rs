use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use utoipa::ToSchema;
use validator::Validate;

use super::Warehouse;

/// Configuration for the space elevator behavior
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceElevatorConfig {
    /// Number of cabins available
    pub cabin_count: usize,
    /// Maximum total quantity per transfer (cabin capacity)
    pub cabin_capacity: u64,
    /// Duration of a transfer in seconds
    pub transfer_duration_secs: u64,
    /// Failure rate parameter (lambda) for exponential distribution
    /// Higher value = more likely to fail
    pub failure_rate: f64,
    /// Duration a cabin is unavailable after failure (repair time) in seconds
    pub repair_duration_secs: u64,
}

impl Default for SpaceElevatorConfig {
    fn default() -> Self {
        Self {
            cabin_count: 3,
            cabin_capacity: 100,
            transfer_duration_secs: 5,
            failure_rate: 0.1, // ~10% chance of failure per transfer
            repair_duration_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CabinState {
    Available,
    InUse,
    UnderRepair,
}

#[derive(Debug, Clone)]
pub struct Cabin {
    pub id: usize,
    pub state: CabinState,
    /// When the cabin will become available again (for InUse or UnderRepair states)
    pub available_at: Option<Instant>,
}

impl Cabin {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            state: CabinState::Available,
            available_at: None,
        }
    }

    pub fn is_available(&self) -> bool {
        match self.state {
            CabinState::Available => true,
            CabinState::InUse | CabinState::UnderRepair => {
                if let Some(available_at) = self.available_at {
                    Instant::now() >= available_at
                } else {
                    false
                }
            }
        }
    }

    pub fn start_transfer(&mut self, duration: Duration) {
        self.state = CabinState::InUse;
        self.available_at = Some(Instant::now() + duration);
    }

    pub fn start_repair(&mut self, duration: Duration) {
        self.state = CabinState::UnderRepair;
        self.available_at = Some(Instant::now() + duration);
    }

    pub fn release(&mut self) {
        self.state = CabinState::Available;
        self.available_at = None;
    }

    /// Try to release the cabin if its timer has expired.
    /// Returns true if the cabin was released.
    pub fn try_release(&mut self) {
        if self.is_available() && self.available_at.is_some() {
            self.release();
        }
    }
}

/// Serializable cabin status for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CabinStatus {
    pub id: usize,
    pub state: CabinState,
    pub available_in_secs: Option<u64>,
}

impl From<&Cabin> for CabinStatus {
    fn from(cabin: &Cabin) -> Self {
        let available_in_secs = cabin.available_at.map(|at| {
            let now = Instant::now();
            if at > now {
                (at - now).as_secs()
            } else {
                0
            }
        });
        Self {
            id: cabin.id,
            state: cabin.state,
            available_in_secs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceElevator {
    pub warehouse: Warehouse,
    pub config: SpaceElevatorConfig,
    #[serde(skip)]
    #[schema(ignore)]
    pub cabins: Vec<Cabin>,
}

/// Error type for space elevator operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum SpaceElevatorError {
    #[error("No cabin available for transfer")]
    NoCabinAvailable,
    #[error("Insufficient stock: {good_name} (requested: {requested}, available: {available})")]
    InsufficientStock {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Transfer exceeds cabin capacity: {total} > {capacity}")]
    ExceedsCapacity { total: u64, capacity: u64 },
    #[error("Transfer must contain at least one item")]
    EmptyTransfer,
}

impl SpaceElevator {
    /// Ensure cabins are initialized based on config.
    /// This should be called after deserialization since cabins are skipped during serde.
    pub fn ensure_cabins_initialized(&mut self) {
        if self.cabins.is_empty() {
            self.cabins = (0..self.config.cabin_count)
                .map(|id| Cabin::new(id))
                .collect();
        }
    }

    /// Try to acquire a cabin for transfer. Returns cabin_id if successful.
    pub fn try_acquire_cabin(&mut self) -> Result<usize, SpaceElevatorError> {
        // Update cabin states (release any that have completed)
        for cabin in &mut self.cabins {
            cabin.try_release();
        }

        // Find an available cabin
        let cabin = self
            .cabins
            .iter_mut()
            .find(|c| c.is_available())
            .ok_or(SpaceElevatorError::NoCabinAvailable)?;

        let cabin_id = cabin.id;
        let duration = Duration::from_secs(self.config.transfer_duration_secs);
        cabin.start_transfer(duration);

        Ok(cabin_id)
    }

    /// Check if transfer failed using exponential distribution.
    /// P(failure) = 1 - e^(-lambda) where lambda = failure_rate
    pub fn check_transfer_failure(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random: f64 = rng.r#gen();
        random < (1.0 - (-self.config.failure_rate).exp())
    }

    /// Complete a transfer, updating cabin state based on success/failure.
    pub fn complete_transfer(&mut self, cabin_id: usize, failed: bool) {
        if let Some(cabin) = self.cabins.iter_mut().find(|c| c.id == cabin_id) {
            if failed {
                cabin.start_repair(Duration::from_secs(self.config.repair_duration_secs));
            } else {
                cabin.release();
            }
        }
    }

    /// Get the transfer duration in seconds
    pub fn transfer_duration_secs(&self) -> u64 {
        self.config.transfer_duration_secs
    }

    /// Get status for API response
    pub fn status(&self) -> SpaceElevatorStatus {
        SpaceElevatorStatus {
            warehouse: self.warehouse.clone(),
            config: self.config.clone(),
            cabins: self.cabins.iter().map(CabinStatus::from).collect(),
        }
    }
}

/// Serializable space elevator status for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpaceElevatorStatus {
    pub warehouse: Warehouse,
    pub config: SpaceElevatorConfig,
    pub cabins: Vec<CabinStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    /// Station -> SpaceElevator -> Warehouse -> Planet economy (export from station)
    ToSurface,
    /// Planet economy -> Warehouse -> SpaceElevator -> Station (import to station)
    ToOrbit,
}

/// A single item in a transfer (good name and quantity)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct TransferItem {
    #[validate(length(min = 1, max = 64))]
    pub good_name: String,
    #[validate(range(min = 1))]
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct TransferRequest {
    pub direction: TransferDirection,
    /// List of goods to transfer
    #[validate(length(min = 1))]
    #[validate(nested)]
    pub items: Vec<TransferItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransferResult {
    pub success: bool,
    pub cabin_id: usize,
    pub duration_secs: u64,
    /// Items that were transferred (or attempted)
    pub items: Vec<TransferItem>,
    /// Total quantity transferred
    pub total_quantity: u64,
    /// If failed, contains failure reason
    pub failure_reason: Option<String>,
}
