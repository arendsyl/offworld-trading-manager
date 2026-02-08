# Space Elevator

The space elevator transfers goods between a station's orbital inventory and a planet-side warehouse. Each connected planet has one space elevator with multiple cabins that operate independently.

## Endpoints

### `GET /settlements/{system_name}/{planet_id}/space-elevator`

Get the current status of the space elevator, including warehouse inventory and cabin states.

**Response (200)**

```json
{
  "warehouse": {
    "owner_id": "alpha-team",
    "inventory": {
      "food": 50,
      "water": 30
    }
  },
  "config": {
    "cabin_count": 3,
    "cabin_capacity": 100,
    "transfer_duration_secs": 5,
    "failure_rate": 0.1,
    "repair_duration_secs": 30
  },
  "cabins": [
    { "id": 0, "state": "available", "available_in_secs": null },
    { "id": 1, "state": "in_use", "available_in_secs": 3 },
    { "id": 2, "state": "under_repair", "available_in_secs": 25 }
  ]
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You do not own this station |
| 404 | System, planet, or settlement not found; or planet not connected |

---

### `POST /settlements/{system_name}/{planet_id}/space-elevator/transfer`

Transfer goods between the station (orbit) and the warehouse (surface). This call blocks for the duration of the transfer.

**Request body**

```json
{
  "direction": "to_surface",
  "items": [
    { "good_name": "iron_ore", "quantity": 50 },
    { "good_name": "electronics", "quantity": 20 }
  ]
}
```

| Field | Description |
|-------|-------------|
| `direction` | `"to_surface"` (station to warehouse) or `"to_orbit"` (warehouse to station) |
| `items` | Array of goods and quantities to transfer |

**Response (200)**

```json
{
  "success": true,
  "cabin_id": 0,
  "duration_secs": 5,
  "items": [
    { "good_name": "iron_ore", "quantity": 50 },
    { "good_name": "electronics", "quantity": 20 }
  ],
  "total_quantity": 70,
  "failure_reason": null
}
```

On failure:

```json
{
  "success": false,
  "cabin_id": 0,
  "duration_secs": 5,
  "items": [
    { "good_name": "iron_ore", "quantity": 50 }
  ],
  "total_quantity": 50,
  "failure_reason": "Cabin malfunction during transfer. Goods returned to source."
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Empty transfer (no items) |
| 400 | Total quantity exceeds cabin capacity |
| 400 | Insufficient stock in source inventory |
| 400 | Storage full (to-orbit only: station would exceed `max_storage`) |
| 400 | No cabin available (all in use or under repair) |
| 403 | You do not own this station |
| 404 | System, planet, or settlement not found; or planet not connected |

## Concepts

### Directions

- **to_surface** -- Moves goods from the station (orbit) down to the warehouse (surface).
- **to_orbit** -- Moves goods from the warehouse (surface) up to the station (orbit). Subject to the station's `max_storage` capacity check.

### Warehouse vs station inventory

The warehouse and the station maintain separate inventories. The warehouse sits on the planet surface and is where settlement-level interactions happen. The station orbits above and is where ships dock to load/unload cargo. The space elevator is the only way to move goods between them.

### Cabin system

The elevator has multiple cabins that operate independently:

- **available** -- Ready for a transfer.
- **in_use** -- Currently carrying a load. The `available_in_secs` field shows when it will be free.
- **under_repair** -- Suffered a malfunction and is being repaired.

Each transfer occupies one cabin for the duration. If no cabin is available, the request is rejected.

### Transfer duration and blocking

The HTTP request blocks for `transfer_duration_secs` while the cabin moves. During this time the server lock is released so other operations can proceed. Plan your transfers accordingly -- long-running transfers tie up your HTTP connection.

### Failure and repair

Each transfer has a chance of cabin malfunction based on the `failure_rate` parameter. If a failure occurs:

1. All goods are returned to the **source** inventory (no goods are lost).
2. The cabin enters `under_repair` state for `repair_duration_secs`.
3. The response indicates `success: false` with a `failure_reason`.

Transfers are all-or-nothing: either everything moves or nothing does.

### Capacity

The total quantity across all items in a single transfer cannot exceed `cabin_capacity`. Split large shipments across multiple transfers.

You can add more cabins via the `upgrade-elevator` construction endpoint (see [Construction](./construction.md)).
