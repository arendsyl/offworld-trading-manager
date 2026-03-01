# Offworld Trading Manager — Reactive Java Client

## Context

You are a trader in a distant galaxy. Multiple star systems contain planets with settlements, stations, and space elevators. Your goal: **maximize profit** by managing your stations, trading goods on an interstellar market, shipping cargo between planets, and expanding your infrastructure — all through a REST API.

The server exposes a variety of interaction patterns:

| Pattern | Example | Reactive challenge |
|---|---|---|
| **Synchronous request/response** | `GET /systems`, `POST /market/orders` | Non-blocking HTTP calls, error handling |
| **Blocking HTTP call** | `POST .../space-elevator/transfer` | The server holds the connection open for several seconds. Your client must not block a thread while waiting. |
| **Async background tasks + polling** | Ship lifecycle, construction projects | You must poll `GET /ships/{id}` or `GET /construction/{id}` to detect state transitions and react accordingly. |
| **Server-Sent Events (SSE)** | `GET /market/trades` | Continuous real-time stream of trade events. Your client must consume this reactively. |
| **Webhooks (server-to-client push)** | Ship arrival notifications, construction completion | You must expose an HTTP endpoint (`callback_url`) and react to incoming events. |
| **Tick-based background generation** | Trade requests (`POST /trade`) | Server generates goods every N seconds autonomously; you monitor and manage active requests. |

Your assignment is to build a **Java application using reactive programming** that interacts with this API to run a profitable trading operation.

---

## Server Documentation

Full API documentation is available in the `docs/` folder of the server repository:

- [`docs/authentication.md`](docs/authentication.md) — API keys and admin tokens
- [`docs/galaxy.md`](docs/galaxy.md) — Systems, planets, settlements
- [`docs/stations.md`](docs/stations.md) — Station management and storage
- [`docs/space-elevator.md`](docs/space-elevator.md) — Elevator transfers (blocking pattern)
- [`docs/players.md`](docs/players.md) — Player profile and credentials
- [`docs/shipping.md`](docs/shipping.md) — Ship lifecycle and webhooks
- [`docs/market.md`](docs/market.md) — Order book, orders, and SSE trade stream
- [`docs/construction.md`](docs/construction.md) — Building stations and upgrades
- [`docs/trade.md`](docs/trade.md) — Import/export trade requests
- [`docs/leaderboard.md`](docs/leaderboard.md) — Player rankings

Read these carefully. They describe every endpoint, request/response format, and error case.

---

## Objectives

Build a reactive Java application that accomplishes the following:

### 1. Galaxy Exploration

Discover the galaxy: fetch all systems, their planets, and identify which planets have settlements, stations, and available resources. Build an internal reactive model of the galaxy state.

### 2. Station Operations

Manage your station(s): check inventory, use the space elevator to move goods between surface warehouses and orbital storage. Handle the **blocking transfer endpoint** without tying up threads.

### 3. Market Participation

- Subscribe to the **SSE trade stream** (`GET /market/trades`) to observe market activity in real time.
- Consult the order book (`GET /market/book/{good}`) and price history (`GET /market/prices`).
- Place buy and sell orders (`POST /market/orders`), handling partial fills and cancellations.
- Use the incoming trade data to inform your trading strategy reactively.

### 4. Shipping & Logistics

- Hire trucking ships to move cargo between your stations (`POST /trucking`).
- Implement the full **ship lifecycle**: respond to docking/undocking authorization requests by either polling ship status or reacting to webhook notifications.
- Expose a **callback endpoint** to receive webhook events (ship arrival, docking, completion) and drive your ship management logic from those events.

### 5. Economy Management

- Create trade requests (`POST /trade`) to generate supply or demand at your settlements.
- Monitor active trade requests and cancel or adjust them based on market conditions.

### 6. Infrastructure Expansion (bonus)

- Install new stations on settled planets.
- Found new settlements on uninhabited planets.
- Upgrade stations (docking bays, storage, mass driver) and space elevators.
- Poll construction project status to react on completion.

### 7. Automated Trading Strategy

Tie everything together: implement an **automated loop** that reactively:
- Monitors market prices and trade events (SSE stream).
- Identifies profitable trading opportunities.
- Places orders, arranges shipping, manages elevator transfers.
- Tracks profit via the leaderboard.

---

## Technical Requirements

### Reactive Programming

- All HTTP interactions must be **non-blocking**. You must not use blocking I/O on the main execution paths.
- Use a reactive programming library of your choice (Project Reactor, RxJava, Mutiny, etc.).
- Demonstrate proper use of reactive operators for:
  - **Composition**: chaining dependent API calls (e.g., fetch planet → check station → transfer goods).
  - **Concurrency**: running independent operations in parallel (e.g., querying multiple systems simultaneously).
  - **Error handling**: retries, fallbacks, timeouts on API calls.
  - **Backpressure**: handling the SSE trade stream without overwhelming your processing pipeline.
  - **Scheduling**: periodic polling for ship/construction status updates.
  - **Event-driven reactions**: triggering actions from webhook callbacks or SSE events.

### Webhook Server

Your application must expose an HTTP server to receive webhook callbacks. Register its URL via `PUT /players/{id}` (`callback_url` field). The server will POST JSON events to this URL for:
- Ship arrival at origin/destination
- Ship docking confirmation
- Ship delivery completion
- Construction project completion

### Configuration

Your application should be configurable (server URL, API key, polling intervals, etc.) via a configuration file or environment variables.

---

## Deliverables

1. **Source code** — A complete, buildable Java project.
2. **README** — How to build, configure, and run your application. Which reactive library you chose and why.
3. **Architecture document** — A short description (with diagrams if helpful) of your reactive pipeline architecture: how data flows through your system, where you use which reactive patterns, and how the different interaction modes (sync, polling, SSE, webhooks) are integrated.

---

## Evaluation Criteria

### Correct use of reactive programming
- Non-blocking I/O throughout.
- Appropriate use of reactive operators (map, flatMap, zip, merge, retry, timeout, etc.).
- Proper subscription management and resource cleanup.
- Backpressure handling on streams.

### API integration completeness
- All five interaction patterns are handled (sync, blocking, polling, SSE, webhooks).
- Correct request/response serialization.
- Proper error handling for HTTP errors and edge cases (403, 404, 409, 503, etc.).

### Architecture and code quality
- Clean separation of concerns (API client layer, business logic, reactive pipelines).
- Readable, well-structured code.
- Sensible use of concurrency (parallel calls where appropriate, sequential where necessary).

### Trading strategy
- The application makes autonomous decisions based on market data.
- Demonstrates reactive coordination across multiple subsystems (market observation → order placement → shipping → delivery).

### Documentation
- Clear explanation of the chosen reactive library and patterns.
- Architecture description shows understanding of the reactive data flow.

---

## Getting Started

1. The server admin will create your player account and provide you with:
   - Your `player_id`
   - Your `api_key` (use as `Authorization: Bearer <api_key>`)
   - The server base URL

2. Start by exploring the galaxy:
   ```
   GET /systems
   GET /systems/{name}/planets
   GET /settlements/{system_name}
   ```

3. Check your player profile and set your callback URL:
   ```
   GET /players/{your_id}
   PUT /players/{your_id}  {"callback_url": "http://your-machine:port/webhooks"}
   ```

4. Check your station inventory:
   ```
   GET /settlements/{system}/{planet_id}/station
   ```

5. Subscribe to market trades:
   ```
   GET /market/trades   (SSE stream)
   ```

6. Start trading and shipping!

---

## Important Notes

- **The space elevator transfer is a blocking HTTP call.** The server will hold your connection open for the transfer duration. Your reactive client must handle this without blocking a thread.
- **Ships require active management.** After hiring a trucking ship, you must authorize docking and undocking at each step. If you don't, your ship will wait indefinitely. Use either polling or webhooks (or both) to drive the lifecycle forward.
- **Market orders can be partially filled.** Track your order status and handle partial fills gracefully.
- **Credits are finite.** Buy orders and construction projects deduct credits upfront. Plan your spending.
- **The leaderboard ranks players by profit** (current credits minus initial credits). This is your score.
- **Storage capacity is limited.** Goods exceeding your station's `max_storage` will cause operations to fail. Monitor and upgrade as needed.
