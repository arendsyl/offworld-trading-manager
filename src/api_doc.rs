use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::error::ErrorResponse;
use crate::models::*;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Offworld Trading Manager",
        version = "0.1.0",
        description = "A space trading management server"
    ),
    paths(
        // Systems
        crate::routes::systems::create_system,
        crate::routes::systems::admin_list_systems,
        crate::routes::systems::admin_get_system,
        crate::routes::systems::update_system,
        crate::routes::systems::delete_system,
        crate::routes::systems::player_list_systems,
        crate::routes::systems::player_get_system,
        // Planets
        crate::routes::planets::create_planet,
        crate::routes::planets::list_planets,
        crate::routes::planets::get_planet,
        crate::routes::planets::update_planet,
        crate::routes::planets::delete_planet,
        // Settlements
        crate::routes::settlements::list_settlements_in_system,
        crate::routes::settlements::get_settlement,
        crate::routes::settlements::create_or_update_settlement,
        crate::routes::settlements::delete_settlement,
        // Stations
        crate::routes::stations::admin_get_station,
        crate::routes::stations::admin_create_station,
        crate::routes::stations::admin_delete_station,
        crate::routes::stations::player_get_station,
        // Connections
        crate::routes::connections::list_connections,
        crate::routes::connections::get_connection,
        crate::routes::connections::create_connection,
        crate::routes::connections::update_connection,
        crate::routes::connections::delete_connection,
        // Players
        crate::routes::players::admin_list_players,
        crate::routes::players::admin_get_player,
        crate::routes::players::admin_create_player,
        crate::routes::players::admin_delete_player,
        crate::routes::players::player_get_self,
        crate::routes::players::player_update_self,
        crate::routes::players::player_regenerate_token,
        // Ships
        crate::routes::ships::list_ships,
        crate::routes::ships::get_ship,
        crate::routes::ships::dock_ship,
        crate::routes::ships::undock_ship,
        // Trucking
        crate::routes::trucking::create_trucking,
        // Market
        crate::routes::market::place_order,
        crate::routes::market::list_orders,
        crate::routes::market::get_order,
        crate::routes::market::cancel_order,
        crate::routes::market::get_order_book,
        crate::routes::market::get_prices,
        // Projects (Construction)
        crate::routes::construction::create_project,
        crate::routes::construction::list_projects,
        crate::routes::construction::get_project,
        // Trade
        crate::routes::trade::create_trade_request,
        crate::routes::trade::list_trade_requests,
        crate::routes::trade::get_trade_request,
        crate::routes::trade::cancel_trade_request,
        // Economy
        crate::routes::economy::get_economy,
        crate::routes::economy::get_economy_prices,
        crate::routes::economy::get_economy_demographics,
        crate::routes::economy::get_economy_flows,
        crate::routes::economy::get_economy_stocks,
        // Leaderboard
        crate::routes::leaderboard::get_leaderboard,
        // Space Elevator
        crate::routes::space_elevator::get_space_elevator,
        crate::routes::space_elevator::transfer,
        // Persistence
        crate::routes::persistence::save_snapshot,
        crate::routes::persistence::load_snapshot,
    ),
    components(
        schemas(
            // System
            StarType, Coordinates, System, CreateSystemRequest, UpdateSystemRequest,
            // Planet
            PlanetStatus, ClimateType, GasGiantType, PlanetType, Planet,
            CreatePlanetRequest, UpdatePlanetRequest, PlanetResource,
            // Settlement
            crate::economy::EconomyState, crate::economy::Demographics,
            crate::economy::PlanetEconomyConfig,
            Settlement, CreateSettlementRequest, UpdateSettlementRequest,
            // Station
            Station, CreateStationRequest, UpdateStationRequest,
            // Warehouse
            Warehouse,
            // Space Elevator
            SpaceElevatorConfig, CabinState, CabinStatus, SpaceElevator,
            SpaceElevatorStatus, TransferDirection, TransferItem, TransferRequest, TransferResult,
            // Mass Driver
            MassDriver, ConnectionStatus, MassDriverConnection,
            CreateConnectionRequest, ConnectionAction, UpdateConnectionRequest, PacketItem,
            // Player
            Player, PlayerPublic, PlayerSelfView, LeaderboardEntry,
            CreatePlayerRequest, UpdatePlayerRequest,
            // Ship
            ShipStatus, Ship, CreateTruckingRequest, DockRequest, UndockRequest,
            // Market
            OrderSide, OrderType, OrderStatus, Order, TradeEvent,
            PlaceOrderRequest, OrderBookSummary, PriceLevel,
            // Construction
            ProjectType, ProjectStatus, ConstructionProject, CreateProjectRequest,
            // Trade Request
            TradeDirection, TradeRequestMode, TradeRequestStatus, TradeRequest,
            CreateTradeRequestBody,
            // Economy
            crate::routes::economy::DemographicsResponse,
            crate::routes::economy::FlowsResponse,
            // Persistence
            crate::routes::persistence::PersistenceResponse,
            // Error
            ErrorResponse,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "systems", description = "Star system management"),
        (name = "planets", description = "Planet management"),
        (name = "settlements", description = "Settlement management"),
        (name = "stations", description = "Station management"),
        (name = "connections", description = "Mass driver connections"),
        (name = "players", description = "Player management"),
        (name = "ships", description = "Ship operations"),
        (name = "trucking", description = "Trucking services"),
        (name = "market", description = "Market and trading"),
        (name = "projects", description = "Construction projects"),
        (name = "trade", description = "Trade requests"),
        (name = "economy", description = "Settlement economy simulation"),
        (name = "leaderboard", description = "Player rankings"),
        (name = "space-elevator", description = "Space elevator transfers"),
        (name = "persistence", description = "Game state persistence"),
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .build(),
                ),
            );
            components.add_security_scheme(
                "api_key",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("API Key")
                        .build(),
                ),
            );
        }
    }
}
