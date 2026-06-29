# Escrow Contract — Storage DataKey Reference

This document is the authoritative map of every `DataKey` variant used by the
escrow contract (`contracts/escrow/src/lib.rs`). It describes what each key
stores, its value type, what `unwrap_or` default is used when absent, which
entrypoints write it, and whether it is a lifetime counter or can be drained by
`settle`.

---

## Why everything is `persistent()`

Soroban offers three storage tiers — `instance`, `temporary`, and `persistent`.

- **`instance`** is tied to the contract instance's TTL; it disappears when the
  contract is evicted.
- **`temporary`** has an independent, short TTL and is designed for ephemeral
  state.
- **`persistent`** has an independent, configurable TTL that can be extended; it
  survives contract eviction and is appropriate for state that must outlive any
  single transaction cycle.

The escrow contract stores everything in `persistent()` because:

1. Usage counters (`Usage`, `TotalUsageByAgent`, `TotalRequestsAllTime`) must
   survive between the moment usage is recorded and the moment the off-chain
   settlement loop reads and drains them — a window that can span many ledger
   TTL cycles.
2. Configuration singletons (`Admin`, `Paused`, `SchemaVersion`, rate-limit
   settings) must be available at every call; losing them on eviction would
   brick the contract.
3. Per-service and per-agent flags (`ServiceRegistered`, `AgentAllowed`, etc.)
   are operational state, not ephemeral hints — they must survive indefinitely
   until explicitly removed by an admin entrypoint.

---

## Key cardinality

| Category | Cardinality | Notes |
|---|---|---|
| Singletons | O(1) | One slot per key type, regardless of services or agents |
| Per-service | O(S) | One slot per registered `service_id` Symbol |
| Per-agent | O(A) | One slot per unique agent `Address` |
| Per-(agent, service) pair | O(A × S) | One slot per unique `(agent, service_id)` combination |

In typical deployments the number of services S is small (tens to hundreds) and
is admin-controlled. The per-agent and per-pair cardinality grows with protocol
usage and drives the rent footprint. Off-chain settlement loops must drain
per-pair counters regularly to keep storage costs bounded.

---

## DataKey Reference Table

### Singletons

| DataKey variant | Value type | Default when absent | Written by | Drained by `settle`? |
|---|---|---|---|---|
| `Admin` | `Address` | — (must exist after `init`) | `init`, `accept_admin_transfer` | No — lifetime |
| `PendingAdmin` | `Address` | `None` (Option) | `propose_admin_transfer` | No — removed by `accept_admin_transfer` or `cancel_admin_transfer` |
| `Paused` | `bool` | `false` | `pause`, `unpause` | No — lifetime |
| `SchemaVersion` | `u32` | `1` (implicit v1) | `init`, `migrate_v1_to_v2` | No — lifetime |
| `RequireServiceRegistration` | `bool` | `false` | `set_require_service_registration` | No — lifetime |
| `MaxRequestsPerCall` | `u32` | `u32::MAX` (no cap) | `set_max_requests_per_call` | No — lifetime |
| `MinRequestsPerCall` | `u32` | `0` (no floor) | `set_min_requests_per_call` | No — lifetime |
| `AllowlistEnabled` | `bool` | `false` | `set_allowlist_enabled` | No — lifetime |
| `MaxRequestsPerWindow` | `u32` | `0` (limiter disabled) | `set_max_requests_per_window` | No — lifetime |
| `WindowSeconds` | `u64` | `0` (limiter disabled) | `set_rate_window_seconds` | No — lifetime |
| `TotalRequestsAllTime` | `u64` | `0` | `record_usage` | No — lifetime (never reset) |

### Per-service slots — cardinality O(S)

| DataKey variant | Key parameter | Value type | Default when absent | Written by | Drained by `settle`? |
|---|---|---|---|---|---|
| `ServicePrice(service_id)` | `Symbol` | `i128` (stroops) | `0` (free/unset) | `set_service_price`; removed by `remove_service_price` | No — lifetime |
| `ServiceRegistered(service_id)` | `Symbol` | `bool` | `false` | `register_service`, `register_service_with_metadata`; removed by `unregister_service` | No — lifetime |
| `ServiceDisabled(service_id)` | `Symbol` | `bool` | `false` | `set_service_disabled` | No — lifetime |
| `ServiceMetadata(service_id)` | `Symbol` | `ServiceMetadata { description: String, owner: Address }` | `None` (Option) | `set_service_metadata`, `register_service_with_metadata`, `transfer_service_ownership`; removed by `clear_service_metadata` | No — lifetime |

### Per-agent slots — cardinality O(A)

| DataKey variant | Key parameter | Value type | Default when absent | Written by | Drained by `settle`? |
|---|---|---|---|---|---|
| `AgentAllowed(agent)` | `Address` | `bool` | `false` | `set_agent_allowed` | No — lifetime |
| `AgentBlocked(agent)` | `Address` | `bool` | `false` | `set_agent_blocked` | No — lifetime |
| `TotalUsageByAgent(agent)` | `Address` | `u32` | `0` | `record_usage` | No — lifetime (never reset by `settle`) |
| `RateWindow(agent)` | `Address` | `(u64, u32)` = `(window_start, count)` | `(0, 0)` | `record_usage` (rate-limit path) | No — rolls forward on next call when window expires |

### Per-(agent, service) pair slots — cardinality O(A × S)

| DataKey variant | Key parameters | Value type | Default when absent | Written by | Drained by `settle`? |
|---|---|---|---|---|---|
| `Usage(agent, service_id)` | `Address`, `Symbol` | `u32` | `0` | `record_usage` | **Yes** — reset to `0` by `settle` |
| `LastSettlement(agent, service_id)` | `Address`, `Symbol` | `u64` (ledger timestamp, seconds since Unix epoch) | `None` (Option) | `settle` | No — stamped (not cleared) by `settle` |

---

## Persistent-storage model details

### `Usage(agent, service_id)`

The primary accumulator. Every `record_usage(agent, service_id, requests)` call
adds `requests` to the existing counter via saturating addition. `settle` reads
the current value, computes `usage * price_stroops`, resets the slot to `0`, and
stamps `LastSettlement`. This is the **only** key drained by `settle`.

### `TotalUsageByAgent(agent)` vs `Usage(agent, service_id)`

`TotalUsageByAgent` is a cross-service lifetime counter. `settle` does **not**
touch it — it accumulates forever (saturating at `u32::MAX`). It is intended for
analytics and SLA tiering, not for billing. The per-pair `Usage` counter is the
billing source of truth.

### `RateWindow(agent)` — fixed-window semantics

Stores `(window_start: u64, count: u32)`. On each `record_usage` call (when the
limiter is active):

1. If `now >= window_start + window_seconds`, the window rolls: `window_start =
   now`, `count = 0`.
2. `count` is incremented by `requests` (saturating).
3. If the new `count > MaxRequestsPerWindow`, the call is rejected.
4. Otherwise the updated `(window_start, count)` is persisted.

An agent can never reset its own window early — `window_start` only advances.

### `LastSettlement(agent, service_id)`

Stores the ledger timestamp at which `settle` last drained this pair. Returns
`None` for pairs never settled (distinct from `Some(0)`, which would imply a
genesis-block settlement). Off-chain SLA monitors use this to detect stuck
settlement cycles.

### Schema version

`SchemaVersion` stores a `u32` schema version number, distinct from the compiled
WASM `version()`. It tracks what the persisted state layout looks like. A fresh
v2 `init` stamps `2` directly; a contract migrated from v1 gets `2` written by
`migrate_v1_to_v2`. Reading `get_schema_version()` returns `1` (the implicit
default) for pre-migration contracts.

---

## Security notes

- **`Usage` is the only drained key.** All other keys are either lifetime
  singletons, per-service/per-agent flags, or monotonically growing counters.
  Settlement accounting relies on `Usage` starting at `0` after each `settle`
  call; any code path that writes `Usage` outside of `record_usage` and `settle`
  would break billing invariants.
- **`TotalUsageByAgent` and `TotalRequestsAllTime` are never reset.** Downstream
  analytics must not treat these as settlement-cycle deltas.
- **`AgentBlocked` takes precedence over `AgentAllowed`.** An agent that is both
  blocked and allow-listed is rejected. Implementations relying on the allowlist
  gate must ensure the blocklist is not populated with the same address.
- **Per-pair cardinality drives rent.** A large population of `(agent,
  service_id)` pairs with unsettled usage will accumulate storage rent. The
  off-chain settlement loop should drain pairs regularly to bound persistent
  storage costs.
