# Authentication

All player-facing endpoints require authentication via a Bearer token sent in the `Authorization` HTTP header.

## Header format

```
Authorization: Bearer <api_key>
```

The `api_key` is a unique token assigned to each player. You can find your current key by calling `GET /players/{player_id}` (see [Players](./players.md)).

## Error responses

| Status | Meaning |
|--------|---------|
| 401 Unauthorized | Missing or invalid `Authorization` header |
| 403 Forbidden | Valid token, but you are trying to access a resource owned by another player |

A 401 response looks like:

```json
{
  "error": "Unauthorized"
}
```

## Token management

- **View your token:** `GET /players/{player_id}` returns your `api_key` in the response.
- **Regenerate your token:** `POST /players/{player_id}/regenerate-token` invalidates the current key and returns a new one. All subsequent requests must use the new key. See [Players](./players.md) for details.

## Public endpoints

The following endpoints do not require authentication:

- `GET /leaderboard` -- [Leaderboard](./leaderboard.md)
- `GET /market/book/{good_name}` -- [Market](./market.md)
- `GET /market/prices` -- [Market](./market.md)
- `GET /market/trades` (SSE) -- [Market](./market.md)
