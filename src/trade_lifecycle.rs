use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, debug};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    PlanetStatus, TradeDirection, TradeRequest, TradeRequestMode, TradeRequestStatus,
};
use crate::state::GalaxyState;

pub fn spawn_trade_request_loop(
    trade_requests: Arc<RwLock<HashMap<Uuid, TradeRequest>>>,
    galaxy: Arc<RwLock<GalaxyState>>,
    config: Arc<AppConfig>,
    request_id: Uuid,
) {
    tokio::spawn(async move {
        let tick_duration = Duration::from_secs_f64(config.trade.tick_duration_secs);

        loop {
            tokio::time::sleep(tick_duration).await;

            // Read request snapshot — if not Active, exit
            let snapshot = {
                let requests = trade_requests.read().await;
                match requests.get(&request_id) {
                    Some(r) if r.status == TradeRequestStatus::Active => r.clone(),
                    _ => {
                        debug!(request_id = %request_id, "Trade request no longer active, exiting loop");
                        return;
                    }
                }
            };

            // Check auto-cancel conditions by reading galaxy
            let should_auto_cancel = {
                let galaxy = galaxy.read().await;
                check_auto_cancel(&galaxy, &snapshot)
            };

            if should_auto_cancel {
                let mut requests = trade_requests.write().await;
                if let Some(req) = requests.get_mut(&request_id) {
                    if req.status == TradeRequestStatus::Active {
                        req.status = TradeRequestStatus::AutoCancelled;
                        req.completed_at = Some(now_ms());
                        info!(request_id = %request_id, "Trade request auto-cancelled");
                    }
                }
                return;
            }

            // Compute units to generate this tick
            let units = match snapshot.mode {
                TradeRequestMode::FixedRate => {
                    let remaining = snapshot.total_quantity.unwrap_or(0)
                        .saturating_sub(snapshot.cumulative_generated);
                    snapshot.rate_per_tick.min(remaining)
                }
                TradeRequestMode::Standing => snapshot.rate_per_tick,
                TradeRequestMode::Threshold => snapshot.rate_per_tick,
            };

            if units == 0 {
                // FixedRate with nothing remaining — mark completed
                let mut requests = trade_requests.write().await;
                if let Some(req) = requests.get_mut(&request_id) {
                    if req.status == TradeRequestStatus::Active {
                        req.status = TradeRequestStatus::Completed;
                        req.completed_at = Some(now_ms());
                        info!(request_id = %request_id, "Trade request completed (FixedRate fulfilled)");
                    }
                }
                return;
            }

            // Write galaxy: update economy supply/demand
            {
                let mut galaxy = galaxy.write().await;
                apply_trade_tick(&mut galaxy, &snapshot, units);
            }

            // Write trade_requests: update cumulative + check completion
            {
                let mut requests = trade_requests.write().await;
                if let Some(req) = requests.get_mut(&request_id) {
                    if req.status != TradeRequestStatus::Active {
                        return;
                    }
                    req.cumulative_generated += units;

                    let completed = match req.mode {
                        TradeRequestMode::FixedRate => {
                            req.cumulative_generated
                                >= req.total_quantity.unwrap_or(0)
                        }
                        TradeRequestMode::Standing => false,
                        TradeRequestMode::Threshold => {
                            // Check in next iteration via galaxy read
                            // For now, check if we need to read galaxy to determine
                            false
                        }
                    };

                    if completed {
                        req.status = TradeRequestStatus::Completed;
                        req.completed_at = Some(now_ms());
                        info!(request_id = %request_id, "Trade request completed");
                        return;
                    }
                }
            }

            // For Threshold mode, check if target reached after applying tick
            if snapshot.mode == TradeRequestMode::Threshold {
                let target_reached = {
                    let galaxy = galaxy.read().await;
                    check_threshold_reached(&galaxy, &snapshot)
                };
                if target_reached {
                    let mut requests = trade_requests.write().await;
                    if let Some(req) = requests.get_mut(&request_id) {
                        if req.status == TradeRequestStatus::Active {
                            req.status = TradeRequestStatus::Completed;
                            req.completed_at = Some(now_ms());
                            info!(request_id = %request_id, "Trade request completed (threshold reached)");
                        }
                    }
                    return;
                }
            }
        }
    });
}

fn check_auto_cancel(galaxy: &GalaxyState, request: &TradeRequest) -> bool {
    // Only Standing and Threshold auto-cancel
    if request.mode == TradeRequestMode::FixedRate {
        return false;
    }

    for system in galaxy.systems.values() {
        for planet in &system.planets {
            if planet.id == request.planet_id {
                match &planet.status {
                    PlanetStatus::Connected {
                        space_elevator, ..
                    } => {
                        match request.direction {
                            TradeDirection::Export => {
                                // Auto-cancel when warehouse has none of the good
                                let qty = space_elevator
                                    .warehouse
                                    .inventory
                                    .get(&request.good_name)
                                    .copied()
                                    .unwrap_or(0);
                                return qty == 0;
                            }
                            TradeDirection::Import => {
                                // Surface has unlimited storage — never auto-cancel for imports
                                return false;
                            }
                        }
                    }
                    _ => {
                        // Planet no longer connected — auto-cancel
                        return true;
                    }
                }
            }
        }
    }
    // Planet not found — auto-cancel
    true
}

fn check_threshold_reached(galaxy: &GalaxyState, request: &TradeRequest) -> bool {
    let target = match request.target_level {
        Some(t) => t,
        None => return false,
    };

    for system in galaxy.systems.values() {
        for planet in &system.planets {
            if planet.id == request.planet_id {
                if let PlanetStatus::Connected { settlement, .. } = &planet.status {
                    let current = match request.direction {
                        TradeDirection::Export => settlement
                            .economy
                            .supply
                            .get(&request.good_name)
                            .copied()
                            .unwrap_or(0),
                        TradeDirection::Import => settlement
                            .economy
                            .demand
                            .get(&request.good_name)
                            .copied()
                            .unwrap_or(0),
                    };
                    return current >= target;
                }
            }
        }
    }
    false
}

fn apply_trade_tick(galaxy: &mut GalaxyState, request: &TradeRequest, units: u64) {
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == request.planet_id {
                if let PlanetStatus::Connected {
                    ref mut settlement, ..
                } = planet.status
                {
                    match request.direction {
                        TradeDirection::Export => {
                            *settlement
                                .economy
                                .supply
                                .entry(request.good_name.clone())
                                .or_insert(0) += units;
                        }
                        TradeDirection::Import => {
                            *settlement
                                .economy
                                .demand
                                .entry(request.good_name.clone())
                                .or_insert(0) += units;
                        }
                    }
                }
                return;
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
