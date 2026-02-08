# Players

Players represent participants in the trading game. Each player has credits, an API key for authentication, and optionally a callback URL for receiving webhook notifications.

## Endpoints

### `GET /players/{player_id}`

Retrieve your own player profile.

**Response (200)**

```json
{
  "id": "alpha-team",
  "name": "Alpha Trading Co.",
  "credits": 50000,
  "api_key": "d4f8e2a1-...",
  "callback_url": "https://example.com/webhooks",
  "pulsar_biscuit": "En0KEwoEMTIzNBgDIg..."
}
```

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You can only view your own profile |
| 404 | Player not found |

---

### `PUT /players/{player_id}`

Update your name and/or callback URL. Only the fields you include will be changed.

**Request body**

```json
{
  "name": "New Team Name",
  "callback_url": "https://example.com/new-webhook"
}
```

Both fields are optional. Omit a field to leave it unchanged.

**Response (200)**

Returns the full `PlayerSelfView` (same shape as GET).

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You can only update your own profile |
| 404 | Player not found |

---

### `POST /players/{player_id}/regenerate-token`

Generate a new API key. The old key is immediately invalidated.

**Request body:** none

**Response (200)**

Returns the full `PlayerSelfView` with the new `api_key`.

**Errors**

| Status | Reason |
|--------|--------|
| 403 | You can only regenerate your own token |
| 404 | Player not found |

## Callback URL and webhooks

The `callback_url` field is used for asynchronous webhook notifications. When certain events happen (ship arrivals, construction completions), the server sends a POST request to your callback URL with a JSON payload.

Webhook events are documented in the relevant feature pages:

- Ship lifecycle events: [Shipping](./shipping.md)
- Construction completions: [Construction](./construction.md)

## Pulsar Biscuit

The `pulsar_biscuit` is an authentication token for connecting to the Apache Pulsar messaging system. It grants access to your player-specific mass driver topics for sending and receiving cargo.
