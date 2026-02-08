# Trade Requests

Trade requests automate the generation of supply and demand in a settlement's economy. A trade request runs in the background, adding units to the settlement's supply or demand each tick until it completes or is cancelled.

## Endpoints

### `POST /trade`

Create a new trade request. A background loop starts immediately, generating goods every tick.

**Request body**

```json
{
  "planet_id": "Sol-3",
  "good_name": "iron_ore",
  "direction": "export",
  "mode": "fixed_rate",
  "rate_per_tick": 10,
  "total_quantity": 500,
  "target_level": null
}
```

| Field | Type | Description |
|-------|------|-------------|
| `planet_id` | string | Planet with your connected station |
| `good_name` | string | The commodity to generate supply/demand for |
| `direction` | string | `"export"` (adds to supply) or `"import"` (adds to demand) |
| `mode` | string | `"fixed_rate"`, `"standing"`, or `"threshold"` |
| `rate_per_tick` | number | Units generated per tick (must be > 0) |
| `total_quantity` | number | Total units to generate (required for `fixed_rate`, ignored otherwise) |
| `target_level` | number | Economy level to reach before completing (required for `threshold`, ignored otherwise) |

**Response (201)**

```json
{
  "id": "a1b2c3d4-...",
  "owner_id": "alpha-team",
  "planet_id": "Sol-3",
  "good_name": "iron_ore",
  "direction": "export",
  "mode": "fixed_rate",
  "rate_per_tick": 10,
  "total_quantity": 500,
  "target_level": null,
  "cumulative_generated": 0,
  "status": "active",
  "created_at": 1700000000000,
  "completed_at": null
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 400 | `rate_per_tick` is zero |
| 400 | `total_quantity` required for `fixed_rate` mode |
| 400 | `target_level` required for `threshold` mode |
| 400 | Planet not connected |
| 400 | You do not own the station on this planet |
| 404 | Planet not found |

---

### `GET /trade`

List all your trade requests.

**Response (200)**

```json
[
  {
    "id": "a1b2c3d4-...",
    "owner_id": "alpha-team",
    "planet_id": "Sol-3",
    "good_name": "iron_ore",
    "direction": "export",
    "mode": "fixed_rate",
    "rate_per_tick": 10,
    "total_quantity": 500,
    "target_level": null,
    "cumulative_generated": 120,
    "status": "active",
    "created_at": 1700000000000,
    "completed_at": null
  }
]
```

---

### `GET /trade/{request_id}`

Get a specific trade request by ID.

**Response (200)**

Same shape as one element of the list.

**Errors**

| Status | Reason |
|--------|--------|
| 404 | Trade request not found or not owned by you |

---

### `DELETE /trade/{request_id}`

Cancel an active trade request. The background loop stops on the next tick.

**Response (200)**

Returns the cancelled trade request with `status: "cancelled"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Trade request is not active |
| 404 | Trade request not found or not owned by you |

## Concepts

### Directions

- **export** -- Each tick adds `rate_per_tick` units to the settlement's `economy.supply` for the given good.
- **import** -- Each tick adds `rate_per_tick` units to the settlement's `economy.demand` for the given good.

### Modes

| Mode | Completes when | Required fields |
|------|---------------|-----------------|
| `fixed_rate` | `cumulative_generated` reaches `total_quantity` | `total_quantity` |
| `standing` | Never (runs until cancelled or auto-cancelled) | -- |
| `threshold` | The settlement's supply (export) or demand (import) reaches `target_level` | `target_level` |

### Tick behavior

A background loop runs for each active trade request. Every tick (configurable, default 5 seconds):

1. Check if the request is still active.
2. Check auto-cancel conditions.
3. Compute units to generate (capped by remaining quantity for `fixed_rate`).
4. Update the settlement's economy.
5. Update `cumulative_generated` and check for completion.

### Auto-cancellation

Standing and threshold requests can be automatically cancelled:

- **Export:** Auto-cancelled when the warehouse has zero stock of the good.
- **Import:** Never auto-cancelled (surface storage is unlimited).
- **Planet no longer connected:** Auto-cancelled immediately.

Auto-cancelled requests have `status: "auto_cancelled"`.

### Statuses

| Status | Meaning |
|--------|---------|
| `active` | Background loop is running |
| `completed` | Request finished (fixed_rate fulfilled or threshold reached) |
| `cancelled` | Manually cancelled via `DELETE` |
| `auto_cancelled` | Automatically cancelled due to conditions above |
