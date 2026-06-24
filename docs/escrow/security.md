# Escrow contract security notes

## Emergency-stop (pause) matrix

The contract exposes a single emergency-stop flag (`DataKey::Paused`),
toggled by the admin via `pause()` / `unpause()`. When paused, every
state-changing entrypoint that mutates billing, registry, or
configuration state must reject calls with
`EscrowError::ContractPaused` (`#4`). Lifecycle controls deliberately
bypass the gate so the operator retains control during an incident, and
all read getters remain callable.

The check is centralised in the private helper `ensure_not_paused(env)`
(defined next to `read_flag` / `write_flag`). Every gated entrypoint
calls it immediately after loading the admin and running
`admin.require_auth()`, so a new mutating entrypoint cannot silently
skip the emergency stop.

### State-changing entrypoints

| Entrypoint                         | Respects pause | Rationale                                                            |
| ---------------------------------- | -------------- | ------------------------------------------------------------------- |
| `record_usage`                     | Yes            | Usage accrual is billing-affecting and must halt during an incident. |
| `settle`                           | Yes            | Settlement moves billed value; must halt during an incident.         |
| `set_service_price`                | Yes            | Pricing config mutation.                                             |
| `register_service`                 | Yes            | Registry mutation.                                                   |
| `unregister_service`               | Yes            | Registry mutation.                                                   |
| `set_service_disabled`             | Yes            | Registry/availability mutation.                                     |
| `set_service_metadata`             | Yes            | Registry metadata mutation.                                         |
| `clear_service_metadata`           | Yes            | Registry metadata mutation.                                         |
| `set_agent_allowed`                | Yes            | Allowlist config mutation.                                          |
| `set_allowlist_enabled`            | Yes            | Allowlist config mutation.                                          |
| `set_require_service_registration` | Yes            | Strict-registration config mutation.                               |
| `set_min_requests_per_call`        | Yes            | Per-call bound config mutation.                                     |
| `set_max_requests_per_call`        | Yes            | Per-call bound config mutation.                                     |
| `transfer_service_ownership`       | Yes            | Ownership mutation; gated independently before this consolidation.  |
| `pause`                            | No (bypass)    | Operator must be able to (re-)assert the stop during an incident.   |
| `unpause`                          | No (bypass)    | Operator must be able to lift the stop to recover.                  |
| `propose_admin_transfer`           | No (bypass)    | Admin recovery/rotation must work even while paused.               |
| `accept_admin_transfer`            | No (bypass)    | Admin recovery/rotation must work even while paused.               |
| `cancel_admin_transfer`            | No (bypass)    | Admin recovery/rotation must work even while paused.               |
| `migrate_v1_to_v2`                 | No (bypass)    | Schema migration is an operator recovery action.                    |
| `init`                             | No (bypass)    | One-time bootstrap; runs before any pause is possible.              |

### Read getters

All read-only getters (`get_admin`, `get_usage`, `get_service_price`,
`compute_billing`, `is_paused`, `get_service_metadata`, `version`, and
the rest) remain callable while paused. They do not mutate state, so the
emergency stop does not apply.
