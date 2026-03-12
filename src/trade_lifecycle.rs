use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    Player, PlanetStatus, TradeDirection, TradeRequest, TradeRequestMode, TradeRequestStatus,
};
use crate::state::GalaxyState;

pub fn spawn_trade_request_loop(
    trade_requests: Arc<RwLock<HashMap<Uuid, TradeRequest>>>,
    galaxy: Arc<RwLock<GalaxyState>>,
    players: Arc<RwLock<HashMap<String, Player>>>,
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

            // Check auto-cancel conditions
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

            // For PriceLimit mode, check price condition before generating
            if snapshot.mode == TradeRequestMode::PriceLimit {
                let price_condition_met = {
                    let galaxy = galaxy.read().await;
                    check_price_condition(&galaxy, &snapshot)
                };
                if !price_condition_met {
                    // Price condition not met — skip this tick
                    debug!(request_id = %request_id, "PriceLimit condition not met, skipping tick");
                    continue;
                }
            }

            // Compute units to generate this tick
            let units = match snapshot.mode {
                TradeRequestMode::Total => {
                    let remaining = snapshot
                        .total_quantity
                        .unwrap_or(0)
                        .saturating_sub(snapshot.cumulative_generated);
                    snapshot.rate_per_tick.min(remaining)
                }
                TradeRequestMode::PriceLimit => snapshot.rate_per_tick,
            };

            if units == 0 {
                // Total mode with nothing remaining — mark completed
                let mut requests = trade_requests.write().await;
                if let Some(req) = requests.get_mut(&request_id) {
                    if req.status == TradeRequestStatus::Active {
                        req.status = TradeRequestStatus::Completed;
                        req.completed_at = Some(now_ms());
                        info!(request_id = %request_id, "Trade request completed (Total fulfilled)");
                    }
                }
                return;
            }

            // Write galaxy + players: update economy flow accumulators + warehouse + credits
            {
                let mut galaxy = galaxy.write().await;
                let mut players = players.write().await;
                apply_trade_tick(&mut galaxy, &mut players, &snapshot, units);
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
                        TradeRequestMode::Total => {
                            req.cumulative_generated >= req.total_quantity.unwrap_or(0)
                        }
                        TradeRequestMode::PriceLimit => false, // runs indefinitely
                    };

                    if completed {
                        req.status = TradeRequestStatus::Completed;
                        req.completed_at = Some(now_ms());
                        info!(request_id = %request_id, "Trade request completed");
                        return;
                    }
                }
            }
        }
    });
}

fn check_auto_cancel(galaxy: &GalaxyState, request: &TradeRequest) -> bool {
    for system in galaxy.systems.values() {
        for planet in &system.planets {
            if planet.id == request.planet_id {
                match &planet.status {
                    PlanetStatus::Connected {
                        space_elevator, ..
                    } => {
                        // Import auto-cancels when warehouse is empty of the good
                        if request.direction == TradeDirection::Import {
                            let qty = space_elevator
                                .warehouse
                                .inventory
                                .get(&request.good_name)
                                .copied()
                                .unwrap_or(0);
                            return qty == 0;
                        }
                        return false;
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

/// Check if price condition is met for PriceLimit mode.
fn check_price_condition(galaxy: &GalaxyState, request: &TradeRequest) -> bool {
    let price_limit = match request.price_limit {
        Some(p) => p,
        None => return false,
    };

    for system in galaxy.systems.values() {
        for planet in &system.planets {
            if planet.id == request.planet_id {
                if let PlanetStatus::Connected { settlement, .. } = &planet.status {
                    let current_price = settlement
                        .economy
                        .prices
                        .get(&request.good_name)
                        .copied()
                        .unwrap_or(0.0);
                    return match request.direction {
                        // Import: active while settlement price > price_limit (good is expensive, player supplies)
                        TradeDirection::Import => current_price > price_limit,
                        // Export: active while settlement price < price_limit (good is cheap, player buys)
                        TradeDirection::Export => current_price < price_limit,
                    };
                }
            }
        }
    }
    false
}

fn apply_trade_tick(
    galaxy: &mut GalaxyState,
    players: &mut HashMap<String, Player>,
    request: &TradeRequest,
    units: u64,
) {
    for system in galaxy.systems.values_mut() {
        for planet in &mut system.planets {
            if planet.id == request.planet_id {
                if let PlanetStatus::Connected {
                    ref mut settlement,
                    ref mut space_elevator,
                    ..
                } = planet.status
                {
                    match request.direction {
                        TradeDirection::Export => {
                            // Always accumulate full request for economy price signal
                            *settlement
                                .economy
                                .exports_this_tick
                                .entry(request.good_name.clone())
                                .or_insert(0.0) += units as f64;

                            // Deliver only what the economy can supply (budget from last tick)
                            let can_deliver = settlement
                                .economy
                                .last_exports_fulfilled
                                .get_mut(&request.good_name)
                                .map(|budget| {
                                    let actual = (units as f64).min(*budget);
                                    *budget -= actual;
                                    actual as u64
                                })
                                .unwrap_or(0);

                            if can_deliver > 0 {
                                // Player pays economy price for exported goods
                                let price = settlement
                                    .economy
                                    .prices
                                    .get(&request.good_name)
                                    .copied()
                                    .unwrap_or(1.0);
                                let cost = (can_deliver as f64 * price).round() as i64;

                                if let Some(player) = players.get_mut(&request.owner_id) {
                                    let affordable = if cost > 0 {
                                        let max_units =
                                            (player.credits as f64 / price).floor() as u64;
                                        can_deliver.min(max_units)
                                    } else {
                                        can_deliver
                                    };

                                    if affordable > 0 {
                                        let actual_cost =
                                            (affordable as f64 * price).round() as i64;
                                        player.credits -= actual_cost;
                                        *space_elevator
                                            .warehouse
                                            .inventory
                                            .entry(request.good_name.clone())
                                            .or_insert(0) += affordable;
                                    }
                                }
                            }
                        }
                        TradeDirection::Import => {
                            // Player warehouse -> settlement imports
                            let available = space_elevator
                                .warehouse
                                .inventory
                                .get(&request.good_name)
                                .copied()
                                .unwrap_or(0);
                            let transferred = units.min(available);
                            if transferred > 0 {
                                if let Some(qty) = space_elevator
                                    .warehouse
                                    .inventory
                                    .get_mut(&request.good_name)
                                {
                                    *qty = qty.saturating_sub(transferred);
                                }
                                *settlement
                                    .economy
                                    .imports_this_tick
                                    .entry(request.good_name.clone())
                                    .or_insert(0.0) += transferred as f64;

                                // Player receives economy price for imported goods
                                let price = settlement
                                    .economy
                                    .prices
                                    .get(&request.good_name)
                                    .copied()
                                    .unwrap_or(1.0);
                                let revenue = (transferred as f64 * price).round() as i64;
                                if let Some(player) = players.get_mut(&request.owner_id) {
                                    player.credits += revenue;
                                }
                            }
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
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}
