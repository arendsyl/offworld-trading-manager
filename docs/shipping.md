# Shipping

Ships transport cargo between stations. You can hire a trucking ship to move your own goods, or ships are automatically created when market trades are matched. Each ship follows a two-leg lifecycle: transit to origin, load cargo, transit to destination, unload cargo.

## Endpoints

### `POST /trucking`

Hire a trucking ship to transport cargo between two stations.

**Request body**

```json
{
  "origin_planet_id": "Sol-3",
  "destination_planet_id": "Proxima Centauri-1",
  "cargo": {
    "iron_ore": 50,
    "electronics": 20
  }
}
```

| Field | Description |
|-------|-------------|
| `origin_planet_id` | Planet ID of the origin station (must be connected) |
| `destination_planet_id` | Planet ID of the destination station (must be connected, different from origin) |
| `cargo` | Map of good names to quantities to transport |

**Response (201)**

```json
{
  "id": "a1b2c3d4-...",
  "owner_id": "alpha-team",
  "origin_planet_id": "Sol-3",
  "destination_planet_id": "Proxima Centauri-1",
  "cargo": { "iron_ore": 50, "electronics": 20 },
  "status": "in_transit_to_origin",
  "trade_id": null,
  "trucking_id": "e5f6g7h8-...",
  "fee": 170,
  "created_at": 1700000000000,
  "arrival_at": null,
  "operation_complete_at": null
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Origin and destination are the same planet |
| 400 | Origin or destination station not found or not connected |
| 400 | You do not own the origin station |
| 400 | You do not own the destination station |
| 400 | Insufficient credits to pay the fee |
| 400 | Empty cargo |

---

### `GET /ships`

List your ships, optionally filtered by status.

**Query parameters**

| Param | Type | Description |
|-------|------|-------------|
| `status` | string | Filter by ship status (optional) |

**Response (200)**

```json
[
  {
    "id": "a1b2c3d4-...",
    "owner_id": "alpha-team",
    "origin_planet_id": "Sol-3",
    "destination_planet_id": "Proxima Centauri-1",
    "cargo": { "iron_ore": 50 },
    "status": "awaiting_origin_docking_auth",
    "trade_id": null,
    "trucking_id": "e5f6g7h8-...",
    "fee": 170,
    "created_at": 1700000000000,
    "arrival_at": 1700000005000,
    "operation_complete_at": null
  }
]
```

---

### `GET /ships/{ship_id}`

Get details of a specific ship. You can view a ship if you own it, or if you own the station at its origin or destination.

Polling this endpoint also advances the ship state: if a loading or unloading timer has expired, the status will be updated to the next `awaiting_*_auth` state in the response.

**Response (200)**

Same shape as one element of the ships list.

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You do not own this ship or the origin/destination station |
| 404 | Ship not found |

---

### `PUT /ships/{ship_id}/dock`

Authorize a ship to dock at your station and begin loading or unloading.

**Request body**

```json
{
  "authorized": true
}
```

The `authorized` field must be `true`.

**Response (200)**

Returns the updated ship.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | `authorized` is false |
| 400 | Ship is not in a dockable state |
| 400 | Insufficient cargo in origin station (trucking ships at origin) |
| 403 | You do not own the relevant station |
| 404 | Ship not found |
| 503 | No docking bay available |

---

### `PUT /ships/{ship_id}/undock`

Authorize a ship to undock from your station after loading or unloading is complete.

**Request body**

```json
{
  "authorized": true
}
```

**Response (200)**

Returns the updated ship.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | `authorized` is false |
| 400 | Ship is not in an undockable state |
| 400 | Storage full at destination station (destination undock only) |
| 403 | You do not own the relevant station |
| 404 | Ship not found |

## Ship lifecycle

Every ship follows this sequence of statuses:

```
in_transit_to_origin
    |
    v  (ship arrives at origin -- webhook: OriginDockingRequest)
awaiting_origin_docking_auth
    |
    v  (station owner calls PUT /ships/{id}/dock -- webhook: ShipDocked)
loading
    |
    v  (timer expires, polled via GET or undock)
awaiting_origin_undocking_auth
    |
    v  (station owner calls PUT /ships/{id}/undock)
in_transit
    |
    v  (ship arrives at destination -- webhook: DockingRequest)
awaiting_docking_auth
    |
    v  (station owner calls PUT /ships/{id}/dock -- webhook: ShipDocked)
unloading
    |
    v  (timer expires, polled via GET or undock)
awaiting_undocking_auth
    |
    v  (station owner calls PUT /ships/{id}/undock -- webhook: ShipComplete)
complete
```

Loading and unloading durations depend on the total cargo units and the server's `seconds_per_unit` configuration.

## Fee calculation

Trucking fees are calculated as:

```
fee = base_fee + ceil(total_cargo_units * fee_per_unit)
```

Where `total_cargo_units` is the sum of all quantities in the cargo map. The fee is deducted from the ship owner's credits when the trucking request is created.

## Webhooks

If your `callback_url` is set (see [Players](./players.md)), the server sends POST requests at key lifecycle transitions:

| Event | Sent to | When |
|-------|---------|------|
| `OriginDockingRequest` | Origin station owner | Ship arrives at origin, waiting for dock authorization |
| `DockingRequest` | Destination station owner | Ship arrives at destination, waiting for dock authorization |
| `ShipDocked` | Ship owner | Ship has docked and started loading or unloading |
| `ShipComplete` | Ship owner | Ship has finished and cargo has been delivered |

**OriginDockingRequest payload**

```json
{
  "event": "OriginDockingRequest",
  "ship_id": "a1b2c3d4-...",
  "origin_planet_id": "Sol-3",
  "destination_planet_id": "Proxima Centauri-1",
  "cargo": { "iron_ore": 50 }
}
```

**DockingRequest payload**

```json
{
  "event": "DockingRequest",
  "ship_id": "a1b2c3d4-...",
  "origin_planet_id": "Sol-3",
  "cargo": { "iron_ore": 50 }
}
```

**ShipDocked payload**

```json
{
  "event": "ShipDocked",
  "ship_id": "a1b2c3d4-...",
  "status": "loading"
}
```

The `status` field is `"loading"` at origin or `"unloading"` at destination.

**ShipComplete payload**

```json
{
  "event": "ShipComplete",
  "ship_id": "a1b2c3d4-..."
}
```

## Docking bay rules

Each station has a limited number of docking bays. The following ships count toward occupancy:

- **At origin:** ships in `loading` or `awaiting_origin_undocking_auth`
- **At destination:** ships in `unloading` or `awaiting_undocking_auth`

If all bays are occupied, new dock requests are rejected with a 503 error. Increase capacity via [Construction](./construction.md).

## Cargo timing

- **Trucking ships:** Cargo is deducted from the origin station inventory when the station owner authorizes docking (the `dock` call). The origin station must have sufficient stock at that moment.
- **Trade ships:** Cargo is reserved from the seller's station when the market order is placed. No additional deduction happens at dock time. See [Market](./market.md).

At destination undocking, all cargo is transferred to the destination station inventory, subject to [storage capacity](./stations.md) checks.
