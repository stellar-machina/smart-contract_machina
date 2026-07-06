# Contributing to Stellar Machina Contracts

Thanks for contributing! This guide documents the conventions that keep the
`escrow` contract's on-chain interface stable for downstream client SDKs.
Please read it before opening a pull request that touches contract code.

## The CI gate

Every PR must pass the same three checks CI runs, from the workspace root:

```bash
cargo fmt --all -- --check   # formatting (no diff allowed)
cargo build                  # compiles clean
cargo test                   # all unit tests green
```

A PR that does not pass all three locally will not pass CI and cannot be
merged. Run them before pushing.

### Coverage and campaign expectations

- **95% test coverage** is the target for contract logic. New entrypoints and
  new branches (error paths, idempotency, slot independence) should ship with
  tests in `contracts/escrow/src/test.rs`.
- Contributions are organized as a **96-hour campaign**: scope your PRs so
  each one addresses a single issue and can be reviewed and merged within the
  campaign window. Keep PRs focused — one issue per branch.

## Error codes are append-only

`EscrowError` (in `contracts/escrow/src/lib.rs`) is annotated with
`#[contracterror]` and `#[repr(u32)]`. The numeric codes are part of the
contract's public ABI: client SDKs match on them, so they are **append-only**.

**Rules:**

- **Never renumber** an existing variant.
- **Never reuse** a retired code for a different meaning.
- Add new variants only at the **end**, with the next unused integer.
- Removing a variant is a breaking change; prefer deprecating it in docs and
  leaving the code permanently reserved.

### Current code table

| Code | Variant | Meaning |
|-----:|---------|---------|
| 1 | `AlreadyInitialized` | `init` was already called and the admin is stored. |
| 2 | `RequestsMustBePositive` | `record_usage` was called with `requests == 0` (also reused for a negative price in `set_service_price`). |
| 3 | `NotInitialized` | An admin-gated entrypoint was invoked but the admin is not set. |
| 4 | `ContractPaused` | A state-changing entrypoint was called while `Paused` is `true`. |
| 5 | `NoPendingAdminTransfer` | `accept_admin_transfer` was called but no pending admin is set. |
| 6 | `NotPendingAdmin` | `accept_admin_transfer` was called by the wrong address. |
| 7 | `ServiceNotRegistered` | `record_usage` referenced an unregistered service while strict registration is enabled. |
| 8 | `RequestsExceedsMaxPerCall` | `record_usage` exceeded the configured `MaxRequestsPerCall` cap. |
| 9 | `RequestsBelowMinPerCall` | `record_usage` was below the configured `MinRequestsPerCall` floor. |
| 10 | `AgentNotAllowed` | `record_usage` was called for an agent not on the allowlist while strict allowlisting is enabled. |
| 11 | `MigrationVersionMismatch` | `migrate_v1_to_v2` was called from a non-v1 schema. |
| 12 | `ServiceDisabled` | `record_usage` referenced a service that has been disabled. |

The next new error must use code **13**.

## Event conventions

### Topic names: `symbol_short!` ≤ 9 characters

Event topics are published with `symbol_short!`, which only accepts symbols of
**9 characters or fewer**. Longer literals fail to compile. Keep topic names
short and stable (current topics: `usage`, `settled`, `paused`).

### Events are additive-only

Like error codes, the event surface is consumed off-chain (indexers,
dashboards, settlement loops). Treat it as **additive-only**:

- Do not rename an existing topic.
- Do not change the shape, order, or types of an existing event's payload.
- New information goes into a **new** event, not an altered existing one.

For reference, the existing events and their payloads are:

- `usage` → `(agent, service_id, requests, total)`
- `settled` → `(agent, service_id, requests, billed)`
- `paused` → a bare `bool` (`true` on `pause()`, `false` on `unpause()`)

## Getter-default convention: `unwrap_or`

Read-only getters return a sensible **default** for absent storage rather than
panicking, using `unwrap_or(...)`. This keeps clients from having to special-case
never-written slots. Examples:

- `get_usage` / `get_service_price` / `get_total_usage_by_agent` → `0`
- `get_max_requests_per_call` → `u32::MAX` (no cap)
- `is_paused` / `is_service_registered` / `is_service_disabled` → `false`
- `get_schema_version` → `1` (the implicit pre-migration default)

When the *absence* of a value is itself meaningful (e.g. "never settled" vs.
"settled at genesis"), return `Option<T>` instead — see `get_last_settlement`
and `get_service_metadata`, which return `None`.

## Test conventions

### Panic assertions for typed errors

Tests that exercise an error path assert the exact contract error code with
`#[should_panic]` using the host's error-formatting string:

```rust
#[test]
#[should_panic(expected = "Error(Contract, #N)")]
fn test_some_rejection() {
    // ... trigger the panic_with_error! path ...
}
```

Substitute `N` with the numeric code from the table above (for example,
`Error(Contract, #4)` for `ContractPaused`). This pins the test to the specific
error variant, so an accidental renumbering would fail the suite.

### Event and state assertions

`env.events().all()` only surfaces events from the **most recent** contract
invocation, so read events immediately after the call under test, before any
other contract call (including read-only getters). Compare topics against a
`Vec<Val>` built with `.into_val(&env)`, and decode payload data back into typed
tuples with `data.into_val(&env)`. When exact event matching is awkward, fall
back to asserting observable state plus that the event count increased.

## Pull request checklist

- [ ] One issue per branch; branch from `main`.
- [ ] `cargo fmt --all -- --check`, `cargo build`, `cargo test` all pass.
- [ ] New / changed behavior is covered by tests (aim for 95% coverage).
- [ ] No renumbered/reused error codes; new codes appended only.
- [ ] No renamed/reshaped existing events; new info in new events.
- [ ] Getters default via `unwrap_or` (or return `Option` when absence matters).
