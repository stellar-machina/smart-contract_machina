# Changelog

All notable changes to `Agentpay-contracts` are documented here.

This project uses **append-only error codes** and **additive events**:
- Error codes (`EscrowError`) are never reassigned. New variants are added at
  the end of the enum with the next sequential integer.
- Contract events are additive: new event topics may be introduced but existing
  topic names and schemas are not changed in a way that breaks existing indexers.

---

## [v2] — current

### Contract surface (escrow)

#### Entrypoints

| Entrypoint | Description |
|---|---|
| `init(admin)` | Initialise the contract; stores the admin address and stamps schema version 2. |
| `record_usage(agent, service_id, requests)` | Accumulate `requests` for an `(agent, service_id)` pair, subject to all active guards (pause, registration, allowlist, blocklist, rate limit, per-call min/max). |
| `get_usage(agent, service_id)` | Return the current cumulative request count for a pair. |
| `get_usage_batch(pairs)` | Return counts for up to `MAX_BATCH_READ` pairs in one call. |
| `propose_admin_transfer(new_admin)` | Admin-only: store a pending admin address for a two-step handover. |
| `accept_admin_transfer()` | Pending admin accepts and becomes the active admin. |
| `pause()` / `unpause()` | Admin-only: toggle the global pause flag. |
| `register_service(service_id)` | Admin-only: mark a service ID as registered. |
| `deregister_service(service_id)` | Admin-only: remove a service's registration flag. |
| `enable_service(service_id)` / `disable_service(service_id)` | Admin-only: toggle whether a service accepts new usage. |
| `set_service_metadata(service_id, description, owner)` | Admin or current owner: set human-readable metadata for a service. |
| `clear_service_metadata(service_id)` | Admin-only: remove metadata for a service (idempotent). |
| `transfer_service_ownership(caller, service_id, new_owner)` | Admin or current owner: update the `owner` field in `ServiceMetadata`. |
| `add_agent_to_allowlist(agent)` / `remove_agent_from_allowlist(agent)` | Admin-only: manage the per-agent allowlist. |
| `block_agent(agent)` / `unblock_agent(agent)` | Admin-only: manage the per-agent blocklist. |
| `set_strict_registration(enabled)` | Admin-only: require services to be registered before usage is recorded. |
| `set_strict_allowlist(enabled)` | Admin-only: require agents to be on the allowlist before recording usage. |
| `set_max_requests_per_call(max)` | Admin-only: cap `requests` per `record_usage` call. 0 = disabled. |
| `set_min_requests_per_call(min)` | Admin-only: floor for `requests` per call. 0 = disabled. |
| `set_max_requests_per_window(max)` | Admin-only: per-agent fixed-window rate limit cap. 0 = disabled. |
| `set_rate_window_seconds(seconds)` | Admin-only: fixed-window duration in seconds. 0 = disabled. |
| `migrate_v1_to_v2()` | Admin-only: one-time migration from schema version 1 to 2. |
| `get_schema_version()` | Return the current storage schema version integer. |

#### Events

| Topic | Emitted by | Description |
|---|---|---|
| `usage` | `record_usage` | Each successful usage recording. |
| `owner_chg` | `transfer_service_ownership` | Service ownership change. |
| `svc_meta` | `set_service_metadata` | Service metadata updated. |
| `admin_prop` | `propose_admin_transfer` | New pending admin proposed. |
| `admin_acc` | `accept_admin_transfer` | Pending admin accepted. |

#### Error codes

See [`docs/escrow/errors.md`](docs/escrow/errors.md) for the full error code
table (codes 1–17).

| Code | Variant | Brief description |
|---|---|---|
| 1 | `AlreadyInitialized` | `init` called on an already-initialised contract. |
| 2 | `RequestsMustBePositive` | Zero or negative request count / price. |
| 3 | `NotInitialized` | Admin-gated call before `init`. |
| 4 | `ContractPaused` | State-changing call while paused. |
| 5 | `NoPendingAdminTransfer` | `accept_admin_transfer` with no pending admin. |
| 6 | `NotPendingAdmin` | Wrong caller for `accept_admin_transfer`; also unauthorized metadata caller. |
| 7 | `ServiceNotRegistered` | Unregistered service in strict mode. |
| 8 | `RequestsExceedsMaxPerCall` | `requests` above per-call cap. |
| 9 | `RequestsBelowMinPerCall` | `requests` below per-call floor. |
| 10 | `AgentNotAllowed` | Agent not on allowlist in strict mode. |
| 11 | `MigrationVersionMismatch` | Migration called on non-v1 schema. |
| 12 | `ServiceDisabled` | Usage on a disabled service. |
| 13 | `ServiceMetadataNotFound` | Metadata entrypoint on service with no metadata. |
| 14 | `InvalidAdminProposal` | Proposed new admin is the current admin. |
| 15 | `RateLimitExceeded` | Agent exceeded per-window request cap. |
| 16 | `BatchTooLarge` | `get_usage_batch` pair count above `MAX_BATCH_READ`. |
| 17 | `AgentBlocked` | Agent on the per-agent blocklist. |

---

## Contribution guidelines

When opening a PR that changes the escrow contract:

1. **New error code** — append a new variant to `EscrowError` with the next
   sequential integer, add a `///` doc comment, add a row to
   `docs/escrow/errors.md`, and add a row to the CHANGELOG entry for the
   version being developed.
2. **New event** — add the topic name and description to the events table above.
3. **New entrypoint** — add a row to the entrypoints table.
4. Do **not** renumber existing error codes or rename existing event topics.
