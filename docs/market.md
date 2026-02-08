# Market

The market is an order-book exchange where players buy and sell goods. Orders are matched automatically, and trade ships are spawned to deliver the goods between stations.

## Endpoints

### `POST /market/orders`

Place a buy or sell order.

**Request body**

```json
{
  "good_name": "iron_ore",
  "side": "buy",
  "order_type": "limit",
  "price": 50,
  "quantity": 100,
  "station_planet_id": "Sol-3"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `good_name` | string | The commodity to trade |
| `side` | string | `"buy"` or `"sell"` |
| `order_type` | string | `"limit"` or `"market"` |
| `price` | number | Price per unit (required for limit orders, ignored for market orders) |
| `quantity` | number | Number of units to trade |
| `station_planet_id` | string | Your station's planet ID (must be connected and owned by you) |

**Response (201)**

```json
{
  "id": "a1b2c3d4-...",
  "player_id": "alpha-team",
  "good_name": "iron_ore",
  "side": "buy",
  "order_type": "limit",
  "price": 50,
  "quantity": 100,
  "filled_quantity": 0,
  "status": "open",
  "station_planet_id": "Sol-3",
  "created_at": 1700000000000
}
```

The order may be immediately partially or fully filled. Check `filled_quantity` and `status` in the response.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Price required for limit orders |
| 400 | Station not found, not connected, or not owned by you |
| 400 | Insufficient credits (buy orders) |
| 400 | Insufficient inventory at station (sell orders) |

---

### `GET /market/orders`

List your orders, optionally filtered by status.

**Query parameters**

| Param | Type | Description |
|-------|------|-------------|
| `status` | string | Filter by order status (optional) |

Order statuses: `open`, `partially_filled`, `filled`, `cancelled`

**Response (200)**

```json
[
  {
    "id": "a1b2c3d4-...",
    "player_id": "alpha-team",
    "good_name": "iron_ore",
    "side": "buy",
    "order_type": "limit",
    "price": 50,
    "quantity": 100,
    "filled_quantity": 30,
    "status": "partially_filled",
    "station_planet_id": "Sol-3",
    "created_at": 1700000000000
  }
]
```

---

### `GET /market/orders/{order_id}`

Get a specific order by ID.

**Response (200)**

Same shape as one element of the orders list.

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You do not own this order |
| 404 | Order not found |

---

### `DELETE /market/orders/{order_id}`

Cancel an order. Only unfilled quantity is refunded.

**Response (200)**

Returns the cancelled order with `status: "cancelled"`.

**Errors**

| Status | Reason |
|--------|--------|
| 400 | Order already fully filled (cannot cancel) |
| 403 | You do not own this order |
| 404 | Order not found |

---

### `GET /market/book/{good_name}`

Get the order book for a specific good. This endpoint is public (no authentication required).

**Response (200)**

```json
{
  "good_name": "iron_ore",
  "bids": [
    { "price": 50, "total_quantity": 200, "order_count": 3 }
  ],
  "asks": [
    { "price": 55, "total_quantity": 100, "order_count": 1 }
  ],
  "last_trade_price": 52
}
```

| Field | Description |
|-------|-------------|
| `bids` | Buy orders aggregated by price level, sorted by price descending |
| `asks` | Sell orders aggregated by price level, sorted by price ascending |
| `last_trade_price` | Price of the most recent trade for this good (null if no trades yet) |

---

### `GET /market/prices`

Get the last traded price for all goods. This endpoint is public.

**Response (200)**

```json
{
  "iron_ore": 52,
  "electronics": 120,
  "food": 30
}
```

Returns a map of good names to their last trade price.

---

### `GET /market/trades`

Subscribe to a real-time stream of trade events via Server-Sent Events (SSE). This endpoint is public.

**Response (200, text/event-stream)**

```
data: {"id":"a1b2c3d4-...","good_name":"iron_ore","price":52,"quantity":30,"buyer_id":"alpha-team","seller_id":"beta-team","buyer_station":"Sol-3","seller_station":"Proxima Centauri-1","timestamp":1700000001000}

data: {"id":"e5f6g7h8-...","good_name":"food","price":30,"quantity":50,"buyer_id":"beta-team","seller_id":"alpha-team","buyer_station":"Proxima Centauri-1","seller_station":"Sol-3","timestamp":1700000002000}
```

The stream sends a keep-alive every 15 seconds. If the client falls behind, lagged events are sent to catch up.

## Concepts

### Order types

- **Limit** -- Executes at the specified price or better. Unmatched portion stays on the book until filled or cancelled. Requires a `price` field.
- **Market** -- Executes immediately at the best available price. Any unmatched portion of a market sell order has its goods returned to the station.

### Order sides

- **Buy** -- You want to acquire goods. Credits are reserved when the order is placed.
- **Sell** -- You want to sell goods. Goods are deducted from your station inventory when the order is placed.

### Reservation and refunds

| Side | Order type | Reserved at placement | Refunded on cancel |
|------|------------|----------------------|-------------------|
| Buy | Limit | `price * quantity` credits | `price * remaining_quantity` credits |
| Buy | Market | Credits per matched trade | N/A (market orders fill or fail) |
| Sell | Any | `quantity` goods from station | `remaining_quantity` goods to station |

When a limit buy order matches at a price lower than the order price, the difference is refunded to the buyer.

### Matching engine

Orders are matched immediately when placed. The engine pairs buy and sell orders for the same good when prices overlap (buy price >= sell price). Partial fills are supported -- an order can match against multiple counter-orders.

### Trade ships

When a trade is matched, a ship is automatically spawned to deliver goods from the seller's station to the buyer's station. The ship follows the standard [shipping lifecycle](./shipping.md). If both stations are on the same planet, delivery is immediate.

### Cancellation

Only orders with remaining unfilled quantity can be cancelled. Fully filled orders cannot be cancelled. On cancellation:

- **Buy orders:** Remaining reserved credits are returned to the player.
- **Sell orders:** Remaining reserved goods are returned to the station inventory.
