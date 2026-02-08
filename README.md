# Offworld Trading Manager

A multiplayer space-trading game server where players build stations, ship cargo between planets, and trade goods on a shared market.

Built with Rust, Axum, and Tokio.

## Quick start

```bash
# Build
cargo build

# Run with default config
cargo run

# Run with a seed file and custom port
cargo run -- --seed galaxy.json --port 8080

# Run with verbose logging
cargo run -- -v    # warn
cargo run -- -vv   # info
cargo run -- -vvv  # debug
```

The server listens on `http://localhost:3000` by default.

## Configuration

The server is configured via a TOML file, environment variables, or CLI flags (highest priority wins).

| Environment variable | Default | Description |
|---------------------|---------|-------------|
| `PORT` | 3000 | Server listen port |
| `ADMIN_TOKEN` | `admin-secret-token` | Bearer token for admin endpoints |
| `PULSAR_URL` | `pulsar://localhost:6650` | Apache Pulsar connection URL |
| `BISCUIT_PRIVATE_KEY` | *(dev key)* | Hex-encoded private key for Biscuit tokens |

Pass a custom config file with `--config path/to/config.toml`.

## Architecture

- **Framework:** Axum with Tokio async runtime
- **State:** `Arc<RwLock<>>` for galaxy, players, ships, market orders, construction projects, and trade requests
- **Auth:** Bearer tokens -- admin routes under `/admin`, player routes at root level
- **Background tasks:** `tokio::spawn` for ship transit, construction builds, and trade request tick loops
- **Messaging:** Apache Pulsar for mass driver cargo transfers between stations
- **Tokens:** Biscuit authorization tokens for Pulsar topic access

## API documentation

Detailed documentation for every player-facing endpoint lives in the [`docs/`](docs/) folder:

| Document | Topics |
|----------|--------|
| [Authentication](docs/authentication.md) | Bearer tokens, error codes, public endpoints |
| [Players](docs/players.md) | Profile management, token regeneration, webhooks |
| [Galaxy](docs/galaxy.md) | Systems, planets, settlements, planet types and statuses |
| [Stations](docs/stations.md) | Station inventory, storage capacity, docking bays, mass driver |
| [Space Elevator](docs/space-elevator.md) | Orbital transfers, cabins, failure/repair mechanics |
| [Shipping](docs/shipping.md) | Trucking, ship lifecycle, dock/undock, fee calculation, webhooks |
| [Market](docs/market.md) | Order book, limit/market orders, matching engine, SSE trade stream |
| [Trade Requests](docs/trade.md) | Automated import/export, tick-based economy generation |
| [Construction](docs/construction.md) | Station installation, settlement founding, upgrades |
| [Leaderboard](docs/leaderboard.md) | Player rankings by profit |

## Running tests

```bash
cargo test
```

Some integration tests use [testcontainers](https://github.com/testcontainers/testcontainers-rs) and require Docker to be running.
