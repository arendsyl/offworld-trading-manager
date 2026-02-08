# Galaxy

The galaxy is made up of star systems, each containing planets. Planets can be uninhabited, settled, or fully connected with a station and space elevator. These endpoints let you explore the galaxy and find trading opportunities.

## Systems

### `GET /systems`

List all star systems, optionally filtered by star type.

**Query parameters**

| Param | Type | Description |
|-------|------|-------------|
| `star_type` | string | Filter by star type (optional) |

Star types: `RedDwarf`, `YellowDwarf`, `BlueGiant`, `RedGiant`, `WhiteDwarf`, `Neutron`, `BinarySystem`

**Response (200)**

```json
[
  {
    "name": "Sol",
    "coordinates": { "x": 0.0, "y": 0.0, "z": 0.0 },
    "star_type": "YellowDwarf",
    "planets": [
      {
        "id": "Sol-3",
        "name": "Earth",
        "position": 3,
        "distance_ua": 1.0,
        "planet_type": { "category": "telluric", "climate": "Temperate" },
        "status": { "status": "connected", "settlement": { "..." }, "station": { "..." }, "space_elevator": { "..." } }
      }
    ]
  }
]
```

---

### `GET /systems/{name}`

Get a single star system by name.

**Response (200)**

Same shape as one element of the list above.

**Errors**

| Status | Reason |
|--------|--------|
| 404 | System not found |

## Planets

### `GET /systems/{system_name}/planets`

List all planets in a system, optionally filtered by type.

**Query parameters**

| Param | Type | Description |
|-------|------|-------------|
| `planet_type` | string | `"telluric"` or `"gas_giant"` (optional) |

**Response (200)**

```json
[
  {
    "id": "Sol-3",
    "name": "Earth",
    "position": 3,
    "distance_ua": 1.0,
    "planet_type": {
      "category": "telluric",
      "climate": "Temperate"
    },
    "status": {
      "status": "connected",
      "settlement": {
        "name": "New Berlin",
        "population": 10000,
        "economy": {
          "supply": { "iron_ore": 500 },
          "demand": { "electronics": 200 }
        },
        "founding_goods": { "steel": 100 }
      },
      "station": {
        "name": "Earth Station Alpha",
        "owner_id": "alpha-team",
        "inventory": { "iron_ore": 250 },
        "mass_driver": { "max_channels": 4 },
        "docking_bays": 2,
        "max_storage": 10000
      },
      "space_elevator": {
        "warehouse": {
          "owner_id": "alpha-team",
          "inventory": { "food": 50 }
        },
        "config": {
          "cabin_count": 3,
          "cabin_capacity": 100,
          "transfer_duration_secs": 5,
          "failure_rate": 0.1,
          "repair_duration_secs": 30
        }
      }
    }
  }
]
```

**Errors**

| Status | Reason |
|--------|--------|
| 404 | System not found |

---

### `GET /systems/{system_name}/planets/{planet_id}`

Get a single planet by ID.

**Response (200)**

Same shape as one element of the planet list.

**Errors**

| Status | Reason |
|--------|--------|
| 404 | System or planet not found |

## Settlements

### `GET /settlements/{system_name}`

List all settled or connected planets in a system. Uninhabited planets are excluded.

**Response (200)**

Returns `Vec<Planet>` -- same shape as the planets list, but only planets with a settlement.

**Errors**

| Status | Reason |
|--------|--------|
| 404 | System not found |

---

### `GET /settlements/{system_name}/{planet_id}`

Get settlement details for a specific planet.

**Response (200)**

```json
{
  "name": "New Berlin",
  "population": 10000,
  "economy": {
    "supply": { "iron_ore": 500, "water": 300 },
    "demand": { "electronics": 200, "food": 150 }
  },
  "founding_goods": { "steel": 100, "electronics": 50 }
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 404 | System, planet, or settlement not found (planet is uninhabited) |

## Concepts

### Planet statuses

Planets progress through three statuses:

1. **Uninhabited** -- No settlement, no station. Can be targeted by `found-settlement` construction projects.
2. **Settled** -- Has a settlement with population and economy, but no station. Can be targeted by `install-station` construction projects.
3. **Connected** -- Has a settlement, station, and space elevator. Fully operational for trading.

### Planet types

- **Telluric** -- Rocky planets with a climate type: `Arid`, `Tropical`, `Temperate`, `Arctic`, `Desert`, `Oceanic`, `Volcanic`
- **Gas giant** -- Gas planets with a gas type: `Jovian`, `Saturnian`, `IceGiant`, `HotJupiter`

### Settlement economy

Each settlement has supply and demand maps indicating which goods are produced locally and which are needed. This information is useful for identifying profitable trade routes.

The `founding_goods` map records the extra goods delivered when the settlement was founded (see [Construction](./construction.md)).
