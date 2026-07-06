# Escrow — Entrypoint & Error-Code Reference

Authoritative reference for the Stellar Machina escrow contract
(`contracts/escrow/src/lib.rs`). Cross-checked against the source. Error
codes are **append-only** — never reuse or renumber a variant.

`require_auth` column: principal whose signature is required. "Pause"
column: whether the entrypoint panics with `ContractPaused` (#4) while the
contract is paused.

## Write entrypoints

| Entrypoint | Params | Auth | Pause | Returns | Panics |
|---|---|---|---|---|---|
| `init` | `admin: Address` | `admin` (once) | no | — | #1 AlreadyInitialized |
| `record_usage` | `agent, service_id, requests: u32` | none | yes | `UsageRecord` (new total) | #2, #4, #7, #8, #9, #10, #12 |
| `settle` | `agent, service_id` | admin | yes | `i128` billed | #3, #4 |
| `set_service_price` | `service_id, price_stroops: i128` | admin | no | — | #2 (negative price), #3 |
| `register_service` | `service_id` | admin | no | — | #3 |
| `unregister_service` | `service_id` | admin | no | — | #3 |
| `set_service_disabled` | `service_id, disabled: bool` | admin | no | — | #3 |
| `set_service_metadata` | `service_id, description, owner` | admin | no | — | #3 |
| `clear_service_metadata` | `service_id` | admin | no | — | #3 |
| `transfer_service_ownership` | `caller, service_id, new_owner` | owner or admin | yes | — | #3, #4, #6 (unauthorized), #13 |
| `set_agent_allowed` | `agent, allowed: bool` | admin | no | — | #3 |
| `set_allowlist_enabled` | `enabled: bool` | admin | no | — | #3 |
| `set_min_requests_per_call` | `min_requests: u32` | admin | no | — | #3 |
| `set_max_requests_per_call` | `max_requests: u32` | admin | no | — | #3 |
| `set_require_service_registration` | `required: bool` | admin | no | — | #3 |
| `pause` | — | admin | n/a | — | #3 |
| `unpause` | — | admin | n/a | — | #3 |
| `propose_admin_transfer` | `new_admin` | admin | no | — | #3, #14 (self-target) |
| `cancel_admin_transfer` | — | admin | no | — | #3 |
| `accept_admin_transfer` | `caller` | pending admin | no | — | #5, #6 |
| `migrate_v1_to_v2` | — | admin | no | — | #3, #11 |

## Read entrypoints (no auth, no pause)

`get_admin`, `get_pending_admin`, `get_usage`, `get_service_price`,
`compute_billing`, `get_last_settlement`, `get_total_requests_all_time`,
`get_total_usage_by_agent`, `get_min_requests_per_call`,
`get_max_requests_per_call`, `is_allowlist_enabled`, `is_agent_allowed`,
`is_service_registration_required`, `is_service_registered`,
`is_service_disabled`, `is_paused`, `get_service_metadata`,
`get_schema_version`, `version`.

`compute_billing` saturates at `i128::MAX`; `get_schema_version` defaults
to 1 when absent; counters default to 0; flags default to `false`.

## Error-code catalogue (`EscrowError`)

| Code | Variant | Trigger | Raised by |
|---|---|---|---|
| 1 | AlreadyInitialized | admin already set | `init` |
| 2 | RequestsMustBePositive | `requests == 0` (also reused for negative price) | `record_usage`, `set_service_price` |
| 3 | NotInitialized | admin-gated call before `init` | all admin entrypoints |
| 4 | ContractPaused | called while paused | `record_usage`, `settle`, `transfer_service_ownership` |
| 5 | NoPendingAdminTransfer | accept with nothing pending | `accept_admin_transfer` |
| 6 | NotPendingAdmin | accept by wrong address (also reused for unauthorized ownership transfer) | `accept_admin_transfer`, `transfer_service_ownership` |
| 7 | ServiceNotRegistered | strict mode + unknown service | `record_usage` |
| 8 | RequestsExceedsMaxPerCall | `requests > MaxRequestsPerCall` | `record_usage` |
| 9 | RequestsBelowMinPerCall | `requests < MinRequestsPerCall` | `record_usage` |
| 10 | AgentNotAllowed | allowlist on + agent not allowed | `record_usage` |
| 11 | MigrationVersionMismatch | migrate from non-v1 schema | `migrate_v1_to_v2` |
| 12 | ServiceDisabled | service disabled | `record_usage` |
| 13 | ServiceMetadataNotFound | metadata-scoped call with no metadata | `transfer_service_ownership` |
| 14 | InvalidAdminProposal | propose current admin as new admin | `propose_admin_transfer` |

## Versioning

- `version()` returns the compiled contract version (currently `2`).
- `get_schema_version()` returns the persisted storage-layout version.

See [migrations.md](migrations.md) for the migration workflow.
