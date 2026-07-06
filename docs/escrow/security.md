# Escrow — Authorization Model

This document records the authorization model for the Stellar Machina escrow
contract (`contracts/escrow/src/lib.rs`) and the testing pattern used to
prove it.

## Model

Authorization is hand-rolled per entrypoint. Every privileged (state
-changing) entrypoint loads the stored `Admin` address and calls
`admin.require_auth()` **before** performing any write. If the admin slot
is unset the call panics with `NotInitialized` (#3) — never a silent
partial write. The two-step admin handover instead authorizes the
*caller* principal (`accept_admin_transfer`) or the current admin
(`propose_admin_transfer` / `cancel_admin_transfer`).

`require_auth` is invoked before the first storage `set`, so a failed
authorization can never leave a half-applied state change.

## Authorization matrix

| Entrypoint | Principal required | Pause-gated |
|---|---|---|
| `init` | the `admin` argument (once) | no |
| `record_usage` | none (public, metered) | yes |
| `settle` | admin | yes |
| `set_service_price` | admin | no |
| `register_service` / `unregister_service` | admin | no |
| `set_service_disabled` | admin | no |
| `set_service_metadata` / `clear_service_metadata` | admin | no |
| `transfer_service_ownership` | current owner **or** admin | yes |
| `set_agent_allowed` / `set_allowlist_enabled` | admin | no |
| `set_min_requests_per_call` / `set_max_requests_per_call` | admin | no |
| `set_require_service_registration` | admin | no |
| `pause` / `unpause` | admin | n/a |
| `propose_admin_transfer` / `cancel_admin_transfer` | current admin | no |
| `accept_admin_transfer` | the pending admin (caller) | no |
| `migrate_v1_to_v2` | admin | no |

Read-only entrypoints (`get_*`, `is_*`, `compute_billing`, `version`)
require no authorization.

## Testing pattern

The default test helper uses `env.mock_all_auths()`, which authorizes
every call and therefore can never prove a *missing* signature is
rejected. Negative-authorization tests instead use scoped mocking:

```rust
env.mock_auths(&[MockAuth {
    address: &admin,
    invoke: &MockAuthInvoke {
        contract: &contract_id,
        fn_name: "init",
        args: (admin.clone(),).into_val(env),
        sub_invokes: &[],
    },
}]);
client.init(&admin);
// The init mock does not cover `pause`, so this must panic in require_auth:
client.pause();
```

See `test_i22_*` in `contracts/escrow/src/test.rs` for one such test per
privileged entrypoint, plus a positive control proving the same call
succeeds once the signature is supplied.
