# Leaderboard

The leaderboard ranks all players by cumulative profit. This endpoint is public and does not require authentication.

## Endpoints

### `GET /leaderboard`

Get the player rankings sorted by profit in descending order.

**Response (200)**

```json
[
  {
    "player_id": "alpha-team",
    "player_name": "Alpha Trading Co.",
    "profit": 12500
  },
  {
    "player_id": "beta-team",
    "player_name": "Beta Logistics",
    "profit": 8300
  },
  {
    "player_id": "gamma-team",
    "player_name": "Gamma Corp",
    "profit": -500
  }
]
```

## Profit calculation

Profit is calculated as:

```
profit = credits - initial_credits
```

Where `credits` is the player's current balance and `initial_credits` is the amount they started with. A negative profit means the player has spent more than they have earned.
