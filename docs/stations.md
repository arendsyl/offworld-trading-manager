# Stations

A station is the player-owned infrastructure on a connected planet. It stores your goods, provides docking bays for ships, and houses a mass driver for inter-planetary cargo launches.

## Endpoints

### `GET /settlements/{system_name}/{planet_id}/station`

Retrieve your station on a connected planet. You must own the station.

**Response (200)**

```json
{
  "name": "Earth Station Alpha",
  "owner_id": "alpha-team",
  "inventory": {
    "iron_ore": 250,
    "electronics": 100
  },
  "mass_driver": {
    "max_channels": 4
  },
  "docking_bays": 2,
  "max_storage": 10000
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You do not own this station |
| 404 | System, planet, or station not found; or planet is not connected |

## Station fields

| Field | Description |
|-------|-------------|
| `name` | Human-readable station name |
| `owner_id` | Player ID of the station owner |
| `inventory` | Map of good names to quantities currently stored |
| `mass_driver` | Mass driver configuration (null if not installed) |
| `docking_bays` | Number of docking bays (default: 2) |
| `max_storage` | Maximum total units the station can hold (default: 10000) |

## Storage capacity

The total number of units across all goods in `inventory` cannot exceed `max_storage`. Storage is enforced at these points:

- **Ship undocking at destination** -- cargo is only transferred to the station if the resulting inventory fits within capacity.
- **Space elevator to-orbit transfers** -- goods moving from the warehouse to the station are rejected if they would exceed capacity.
- **Mass driver reception** -- incoming packets are rejected if the station is full.

If a storage check fails, the operation returns a 400 error and no goods are moved.

You can increase storage capacity via the `upgrade-station` construction endpoint (see [Construction](./construction.md)).

## Docking bays

Docking bays limit how many ships can be loading or unloading at your station simultaneously. The occupancy count includes ships in `loading` or `awaiting_origin_undocking_auth` status (at origin) and ships in `unloading` or `awaiting_undocking_auth` status (at destination).

If all bays are occupied, new dock requests will be rejected with a 503 error. You can increase docking bay capacity via [Construction](./construction.md).

## Mass driver

The mass driver enables direct cargo launches between stations via the Pulsar messaging system. The `max_channels` field indicates how many concurrent transfers it supports. Upgrade channels via [Construction](./construction.md).
