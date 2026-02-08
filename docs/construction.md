# Construction

Construction projects let you build new infrastructure and upgrade existing facilities. Projects consume credits and goods upfront, then run asynchronously until completion.

## Endpoints

### `GET /construction`

List all your construction projects.

**Response (200)**

```json
[
  {
    "id": "a1b2c3d4-...",
    "owner_id": "alpha-team",
    "project_type": "install_station",
    "source_planet_id": "Sol-3",
    "target_planet_id": "Proxima Centauri-1",
    "fee": 5000,
    "goods_consumed": { "steel": 100, "electronics": 50 },
    "extra_goods": {},
    "status": "in_transit",
    "created_at": 1700000000000,
    "completion_at": 1700000060000,
    "station_name": "Proxima Station",
    "settlement_name": null
  }
]
```

---

### `GET /construction/{project_id}`

Get a specific construction project by ID.

**Response (200)**

Same shape as one element of the list.

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You do not own this project |
| 404 | Project not found |

---

### `POST /construction/install-station`

Install a station on a settled planet that does not yet have one. This creates a two-phase project (transit then build).

**Request body**

```json
{
  "source_planet_id": "Sol-3",
  "target_planet_id": "Proxima Centauri-1",
  "station_name": "Proxima Station"
}
```

| Field | Description |
|-------|-------------|
| `source_planet_id` | Your connected station providing materials |
| `target_planet_id` | Settled planet to receive the new station |
| `station_name` | Name for the new station |

**Response (201)**

Returns the created `ConstructionProject` with `status: "in_transit"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Source and target are the same planet |
| 400 | Source station not found or not connected |
| 400 | You do not own the source station |
| 400 | Target planet not found |
| 400 | Target already has a station (already connected) |
| 400 | Target is not settled (must be settled first) |
| 400 | Insufficient credits |
| 400 | Insufficient goods at source station |

---

### `POST /construction/found-settlement`

Found a new settlement on an uninhabited planet. This creates a two-phase project (transit then build) and establishes both a settlement and a station.

**Request body**

```json
{
  "source_planet_id": "Sol-3",
  "target_planet_id": "Alpha Centauri-2",
  "settlement_name": "New Eden",
  "station_name": "Eden Station",
  "extra_goods": {
    "food": 100,
    "water": 50
  }
}
```

| Field | Description |
|-------|-------------|
| `source_planet_id` | Your connected station providing materials |
| `target_planet_id` | Uninhabited planet to settle |
| `settlement_name` | Name for the new settlement |
| `station_name` | Name for the new station |
| `extra_goods` | Additional goods to deliver (stored in settlement's `founding_goods`) |

**Response (201)**

Returns the created `ConstructionProject` with `status: "in_transit"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Source and target are the same planet |
| 400 | Source station not found or not connected |
| 400 | You do not own the source station |
| 400 | Target planet not found |
| 400 | Target is not uninhabited |
| 400 | Insufficient credits |
| 400 | Insufficient goods at source station (required goods + extra goods) |

---

### `POST /construction/upgrade-station`

Upgrade a station facility. This creates a single-phase project (build only, no transit).

**Request body**

```json
{
  "planet_id": "Sol-3",
  "upgrade_type": "docking_bays"
}
```

| Field | Description |
|-------|-------------|
| `planet_id` | Planet with your station to upgrade |
| `upgrade_type` | One of: `"docking_bays"`, `"mass_driver_channels"`, `"storage"` |

**Response (201)**

Returns the created `ConstructionProject` with `status: "building"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Station not found or not connected |
| 400 | You do not own this station |
| 400 | No mass driver installed (for `mass_driver_channels` upgrade) |
| 400 | Insufficient credits |
| 400 | Insufficient goods at station |

---

### `POST /construction/upgrade-elevator`

Add a cabin to the space elevator. This creates a single-phase project (build only).

**Request body**

```json
{
  "planet_id": "Sol-3"
}
```

| Field | Description |
|-------|-------------|
| `planet_id` | Planet with your station and space elevator |

**Response (201)**

Returns the created `ConstructionProject` with `status: "building"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Station not found or not connected |
| 400 | You do not own this station |
| 400 | Insufficient credits |
| 400 | Insufficient goods in elevator warehouse |

## Concepts

### Project lifecycle

Projects have two possible lifecycles:

**Two-phase (install-station, found-settlement):**
```
in_transit --> building --> complete
```
The transit phase covers the travel time between source and target planets. The build phase follows.

**Single-phase (upgrade-station, upgrade-elevator):**
```
building --> complete
```
Upgrades happen in place with no transit time.

The `completion_at` timestamp tells you when the project will finish. Both phases are handled automatically by the server.

### Project types

| Type | Description | Lifecycle |
|------|-------------|-----------|
| `install_station` | Build a station on a settled planet | Two-phase |
| `found_settlement` | Create a settlement and station on an uninhabited planet | Two-phase |
| `upgrade_docking_bays` | Add a docking bay to a station | Single-phase |
| `upgrade_mass_driver_channels` | Add a channel to a station's mass driver | Single-phase |
| `upgrade_storage` | Increase station storage capacity | Single-phase |
| `upgrade_elevator_cabins` | Add a cabin to the space elevator | Single-phase |

### Costs

All costs are paid upfront when the project is created:

- **Credits** are deducted from the player's account.
- **Goods** are deducted from the source station's inventory (or the elevator warehouse for elevator upgrades).

If the project fails validation, nothing is deducted.

### Extra goods (found-settlement)

The `extra_goods` map in a found-settlement request specifies additional goods beyond the base construction requirements. These goods are deducted from the source station along with the required materials and are stored in the new settlement's `founding_goods` field (see [Galaxy](./galaxy.md)).

### Completion webhook

When a project completes, a `ConstructionComplete` webhook is sent to your `callback_url` if configured (see [Players](./players.md)).
