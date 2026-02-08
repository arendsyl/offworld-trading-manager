use std::sync::Arc;
use std::time::Duration;

use offworld_trading_manager::config::{AppConfig, PulsarConfig};
use offworld_trading_manager::consumer::spawn_send_consumer;
use offworld_trading_manager::models::{
    ConnectionStatus, MassDriver, MassDriverConnection, PlanetStatus, SendMessage,
};
use offworld_trading_manager::pulsar::PulsarManager;
use offworld_trading_manager::state;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use uuid::Uuid;

const TEST_SEED_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/seed.json");

async fn setup_pulsar_test() -> (
    Arc<tokio::sync::RwLock<state::GalaxyState>>,
    Arc<PulsarManager>,
    Arc<AppConfig>,
    ContainerAsync<testcontainers_modules::pulsar::Pulsar>,
) {
    let container = testcontainers_modules::pulsar::Pulsar::default()
        .start()
        .await
        .expect("Failed to start Pulsar container");

    let port = container
        .get_host_port_ipv4(6650)
        .await
        .expect("Failed to get Pulsar mapped port");

    let mut config = AppConfig::default();
    config.pulsar = PulsarConfig {
        url: format!("pulsar://127.0.0.1:{}", port),
        ..PulsarConfig::default()
    };

    let galaxy = state::create_galaxy_state_from_file(TEST_SEED_FILE)
        .expect("Failed to load seed data");

    // Setup Mars station
    {
        let mut g = galaxy.write().await;
        let system = g.systems.get_mut("Sol").unwrap();
        let mars = system.planets.iter_mut().find(|p| p.id == "Sol-4").unwrap();
        if let PlanetStatus::Settled { settlement } = &mars.status {
            use offworld_trading_manager::models::{
                Cabin, SpaceElevator, SpaceElevatorConfig, Station, Warehouse,
            };
            let se_config = SpaceElevatorConfig::default();
            let cabins = (0..se_config.cabin_count).map(Cabin::new).collect();
            mars.status = PlanetStatus::Connected {
                settlement: settlement.clone(),
                station: Station {
                    name: "Mars Orbital".to_string(),
                    owner_id: "test".to_string(),
                    inventory: Default::default(),
                    mass_driver: Some(MassDriver::new(4)),
                    docking_bays: 2,
                    max_storage: u64::MAX,
                },
                space_elevator: SpaceElevator {
                    warehouse: Warehouse {
                        owner_id: "test".to_string(),
                        inventory: Default::default(),
                    },
                    config: se_config,
                    cabins,
                },
            };
        }
    }

    let pulsar = PulsarManager::new(config.pulsar.clone())
        .await
        .expect("Failed to connect to Pulsar");

    (galaxy, Arc::new(pulsar), Arc::new(config), container)
}

#[tokio::test]
#[ignore]
async fn test_pulsar_send_packet_transfers_inventory() {
    let (galaxy, pulsar, config, _container) = setup_pulsar_test().await;

    // Add inventory to Earth station
    {
        let mut g = galaxy.write().await;
        let system = g.systems.get_mut("Sol").unwrap();
        let earth = system.planets.iter_mut().find(|p| p.id == "Sol-3").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut earth.status {
            station.inventory.insert("iron_ore".to_string(), 100);
        }
    }

    // Create an active connection
    let connection_id = Uuid::new_v4();
    {
        let mut g = galaxy.write().await;
        g.connections.insert(
            connection_id,
            MassDriverConnection {
                id: connection_id,
                system: "Sol".to_string(),
                from_planet: "Sol-3".to_string(),
                to_planet: "Sol-4".to_string(),
                status: ConnectionStatus::Active,
            },
        );
    }

    // Spawn consumer for Earth (from_planet)
    let _handle = spawn_send_consumer(
        galaxy.clone(),
        pulsar.clone(),
        config.clone(),
        "alpha-team".to_string(),
    );

    // Give consumer time to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Send a packet
    let msg = SendMessage::Packet {
        connection_id,
        items: vec![offworld_trading_manager::models::PacketItem {
            good_name: "iron_ore".to_string(),
            quantity: 10,
        }],
    };
    let payload = serde_json::to_vec(&msg).unwrap();

    // Produce directly to the send topic
    let topic = pulsar.topic("send", "alpha-team");
    let mut producer = pulsar
        .client
        .producer()
        .with_topic(&topic)
        .build()
        .await
        .unwrap();
    producer.send_non_blocking(payload).await.unwrap();

    // Wait for latency + processing
    // Earth is 1.0 AU, Mars is 1.52 AU, diff = 0.52 AU * 2.0 = 1.04s
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify inventories
    let g = galaxy.read().await;
    let system = g.systems.get("Sol").unwrap();

    let earth = system.planets.iter().find(|p| p.id == "Sol-3").unwrap();
    if let PlanetStatus::Connected { station, .. } = &earth.status {
        assert_eq!(
            station.inventory.get("iron_ore").copied().unwrap_or(0),
            90,
            "Earth should have 90 iron_ore remaining"
        );
    }

    let mars = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();
    if let PlanetStatus::Connected { station, .. } = &mars.status {
        assert_eq!(
            station.inventory.get("iron_ore").copied().unwrap_or(0),
            10,
            "Mars should have received 10 iron_ore"
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_pulsar_packet_rejected_oversized() {
    let (galaxy, pulsar, config, _container) = setup_pulsar_test().await;

    // Add inventory to Earth
    {
        let mut g = galaxy.write().await;
        let system = g.systems.get_mut("Sol").unwrap();
        let earth = system.planets.iter_mut().find(|p| p.id == "Sol-3").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut earth.status {
            station.inventory.insert("iron_ore".to_string(), 100);
        }
    }

    let connection_id = Uuid::new_v4();
    {
        let mut g = galaxy.write().await;
        g.connections.insert(
            connection_id,
            MassDriverConnection {
                id: connection_id,
                system: "Sol".to_string(),
                from_planet: "Sol-3".to_string(),
                to_planet: "Sol-4".to_string(),
                status: ConnectionStatus::Active,
            },
        );
    }

    let _handle = spawn_send_consumer(
        galaxy.clone(),
        pulsar.clone(),
        config.clone(),
        "alpha-team".to_string(),
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Send oversized packet (max is 20)
    let msg = SendMessage::Packet {
        connection_id,
        items: vec![offworld_trading_manager::models::PacketItem {
            good_name: "iron_ore".to_string(),
            quantity: 50,
        }],
    };
    let payload = serde_json::to_vec(&msg).unwrap();

    let topic = pulsar.topic("send", "alpha-team");
    let mut producer = pulsar
        .client
        .producer()
        .with_topic(&topic)
        .build()
        .await
        .unwrap();
    producer.send_non_blocking(payload).await.unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Verify inventory was NOT deducted
    let g = galaxy.read().await;
    let system = g.systems.get("Sol").unwrap();
    let earth = system.planets.iter().find(|p| p.id == "Sol-3").unwrap();
    if let PlanetStatus::Connected { station, .. } = &earth.status {
        assert_eq!(
            station.inventory.get("iron_ore").copied().unwrap_or(0),
            100,
            "Earth should still have 100 iron_ore (packet was rejected)"
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_pulsar_packet_rejected_insufficient_inventory() {
    let (galaxy, pulsar, config, _container) = setup_pulsar_test().await;

    // Earth has no iron_ore
    let connection_id = Uuid::new_v4();
    {
        let mut g = galaxy.write().await;
        g.connections.insert(
            connection_id,
            MassDriverConnection {
                id: connection_id,
                system: "Sol".to_string(),
                from_planet: "Sol-3".to_string(),
                to_planet: "Sol-4".to_string(),
                status: ConnectionStatus::Active,
            },
        );
    }

    let _handle = spawn_send_consumer(
        galaxy.clone(),
        pulsar.clone(),
        config.clone(),
        "alpha-team".to_string(),
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    let msg = SendMessage::Packet {
        connection_id,
        items: vec![offworld_trading_manager::models::PacketItem {
            good_name: "unobtainium".to_string(),
            quantity: 5,
        }],
    };
    let payload = serde_json::to_vec(&msg).unwrap();

    let topic = pulsar.topic("send", "alpha-team");
    let mut producer = pulsar
        .client
        .producer()
        .with_topic(&topic)
        .build()
        .await
        .unwrap();
    producer.send_non_blocking(payload).await.unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Mars should have received nothing
    let g = galaxy.read().await;
    let system = g.systems.get("Sol").unwrap();
    let mars = system.planets.iter().find(|p| p.id == "Sol-4").unwrap();
    if let PlanetStatus::Connected { station, .. } = &mars.status {
        assert_eq!(
            station.inventory.get("unobtainium").copied().unwrap_or(0),
            0,
            "Mars should have no unobtainium"
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_pulsar_packet_rejected_inactive_connection() {
    let (galaxy, pulsar, config, _container) = setup_pulsar_test().await;

    {
        let mut g = galaxy.write().await;
        let system = g.systems.get_mut("Sol").unwrap();
        let earth = system.planets.iter_mut().find(|p| p.id == "Sol-3").unwrap();
        if let PlanetStatus::Connected { station, .. } = &mut earth.status {
            station.inventory.insert("iron_ore".to_string(), 100);
        }
    }

    // Create a PENDING (not active) connection
    let connection_id = Uuid::new_v4();
    {
        let mut g = galaxy.write().await;
        g.connections.insert(
            connection_id,
            MassDriverConnection {
                id: connection_id,
                system: "Sol".to_string(),
                from_planet: "Sol-3".to_string(),
                to_planet: "Sol-4".to_string(),
                status: ConnectionStatus::Pending,
            },
        );
    }

    let _handle = spawn_send_consumer(
        galaxy.clone(),
        pulsar.clone(),
        config.clone(),
        "alpha-team".to_string(),
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    let msg = SendMessage::Packet {
        connection_id,
        items: vec![offworld_trading_manager::models::PacketItem {
            good_name: "iron_ore".to_string(),
            quantity: 5,
        }],
    };
    let payload = serde_json::to_vec(&msg).unwrap();

    let topic = pulsar.topic("send", "alpha-team");
    let mut producer = pulsar
        .client
        .producer()
        .with_topic(&topic)
        .build()
        .await
        .unwrap();
    producer.send_non_blocking(payload).await.unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Inventory should be unchanged
    let g = galaxy.read().await;
    let system = g.systems.get("Sol").unwrap();
    let earth = system.planets.iter().find(|p| p.id == "Sol-3").unwrap();
    if let PlanetStatus::Connected { station, .. } = &earth.status {
        assert_eq!(
            station.inventory.get("iron_ore").copied().unwrap_or(0),
            100,
            "Earth should still have 100 iron_ore (connection not active)"
        );
    }
}
