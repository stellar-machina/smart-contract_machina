# Design Document: Config Change Events

## Overview

Six administrative setters in the AgentPay escrow contract (`set_max_requests_per_call`, `set_min_requests_per_call`, `set_max_requests_per_window`, `set_rate_window_seconds`, `set_require_service_registration`, `set_allowlist_enabled`) currently mutate operational policy silently. Unlike `set_service_price` (emits `price_set`) and `pause`/`unpause` (emit `paused`), these setters provide no on-chain signal. An indexer or security monitor therefore has no way to observe when an operator tightens or loosens limits.

This feature adds a single additive `cfg_set` event to each of the six setters, closing the observability gap with zero changes to existing event payloads or logic ordering.

## Architecture

The change is purely additive: a single `env.events().publish(...)` call is appended to the body of each config setter, after the existing storage write and after all validation and auth checks pass. No new data keys, error codes, or contract entrypoints are introduced.

```
Admin caller
     │
     ▼
require_admin(&env)          ← unchanged (panics #3 on non-admin)
     │
     ▼
env.storage().persistent()   ← unchanged write (same position, same args)
     │
     ▼
env.events().publish(        ← NEW: emitted after write, same call frame
  (symbol_short!("cfg_set"),),
  (key_symbol, new_value)
)
```

Because Soroban event publication is infallible once the host accepts the call (it does not return an error value), the storage write always precedes the event. If the host rejects the call at auth time, neither the write nor the event occurs — which is the desired "no event on error path" behavior verified by existing `require_admin` semantics.

## Components and Interfaces

### Affected entrypoints

All six entrypoints remain admin-gated (unchanged). The only modification to each is the appended `publish` call.

| Entrypoint | Config key | Value type | Symbol length |
|---|---|---|---|
| `set_max_requests_per_call` | `max_call` | `u32` | 8 |
| `set_min_requests_per_call` | `min_call` | `u32` | 8 |
| `set_max_requests_per_window` | `max_win` | `u32` | 7 |
| `set_rate_window_seconds` | `win_sec` | `u64` | 7 |
| `set_require_service_registration` | `svc_reg` | `bool` | 7 |
| `set_allowlist_enabled` | `allowlist` | `bool` | 9 |

All key symbols are ≤ 9 characters, satisfying the `symbol_short!` macro constraint at compile time.

### Event schema

Every config-change event follows this structure:

```
topic:  (symbol_short!("cfg_set"),)
data:   (symbol_short!("<config_key>"), <new_value>)
```

The data tuple is always two elements: a `Symbol` (the config key) and the typed new value. A subscriber can decode all six config events with a single `cfg_set` topic filter and then branch on the first data element to obtain the typed second element:

```rust
// Pseudocode subscriber decoder
match key {
    "max_call"  => u32::from_val(value),
    "min_call"  => u32::from_val(value),
    "max_win"   => u32::from_val(value),
    "win_sec"   => u64::from_val(value),
    "svc_reg"   => bool::from_val(value),
    "allowlist" => bool::from_val(value),
}
```

### Unchanged entrypoints and events

The following existing events are **not modified**:

| Topic | Emitted by | Payload |
|---|---|---|
| `price_set` | `set_service_price` | `(service_id: Symbol, price: i128)` |
| `price_rm` | `remove_service_price` | `service_id: Symbol` |
| `paused` | `pause`, `unpause` | `bool` |
| `settled` | `settle` | `(agent, service_id, requests: u32, billed: i128)` |
| `usage` | `record_usage` | `(agent, service_id, delta: u32, total: u32)` |
| `usage_hi` | `record_usage` | `(agent, service_id, total: u32)` |
| `usage_dec` | `decrement_usage` | `(agent, service_id, amount: u32, new_total: u32)` |
| `tiers_set` | `set_price_tiers` | `service_id: Symbol` |
| `tiers_rm` | `remove_price_tiers` | `service_id: Symbol` |
| `owner_chg` | `transfer_service_ownership` | `(service_id, old_owner, new_owner)` |
| `meta_clr` | `clear_service_metadata` | `service_id: Symbol` |

## Data Models

No new `DataKey` variants, `contracttype` structs, or error codes are introduced. The event data is constructed inline at each call site using existing SDK primitives.

### Event data types

The Soroban SDK encodes `Val` types. The data tuple `(Symbol, T)` encodes as two `Val` entries in the event. Subscribers decode using `into_val`:

- `(Symbol, u32)` — for `max_call`, `min_call`, `max_win`
- `(Symbol, u64)` — for `win_sec`
- `(Symbol, bool)` — for `svc_reg`, `allowlist`

There is no ambiguity at the protocol level: the config key in position 0 determines the type of the value in position 1.

### Implementation pattern (same for all six setters)

```rust
// Example: set_max_requests_per_call (u32 variant)
pub fn set_max_requests_per_call(env: Env, max_requests: u32) {
    require_admin(&env);
    env.storage()
        .persistent()
        .set(&DataKey::MaxRequestsPerCall, &max_requests);
    env.events().publish(
        (symbol_short!("cfg_set"),),
        (symbol_short!("max_call"), max_requests),
    );
}

// Example: set_require_service_registration (bool variant)
pub fn set_require_service_registration(env: Env, required: bool) {
    require_admin(&env);
    write_flag(&env, &DataKey::RequireServiceRegistration, required);
    env.events().publish(
        (symbol_short!("cfg_set"),),
        (symbol_short!("svc_reg"), required),
    );
}
```

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: `set_max_requests_per_call` always emits the correct cfg_set event

*For any* `u32` value `v`, calling `set_max_requests_per_call(v)` on an initialized contract shall emit exactly one event with topic `cfg_set` and data `(max_call, v)`, and the stored value shall equal `v`.

**Validates: Requirements 1.1, 1.2, 1.4**

### Property 2: `set_min_requests_per_call` always emits the correct cfg_set event

*For any* `u32` value `v`, calling `set_min_requests_per_call(v)` shall emit exactly one event with topic `cfg_set` and data `(min_call, v)`, and the stored value shall equal `v`.

**Validates: Requirements 2.1, 2.2, 2.4**

### Property 3: `set_max_requests_per_window` always emits the correct cfg_set event

*For any* `u32` value `v`, calling `set_max_requests_per_window(v)` shall emit exactly one event with topic `cfg_set` and data `(max_win, v)`, and the stored value shall equal `v`.

**Validates: Requirements 3.1, 3.3**

### Property 4: `set_rate_window_seconds` always emits the correct cfg_set event

*For any* `u64` value `v`, calling `set_rate_window_seconds(v)` shall emit exactly one event with topic `cfg_set` and data `(win_sec, v)`, and the stored value shall equal `v`.

**Validates: Requirements 4.1, 4.4**

### Property 5: `set_require_service_registration` always emits the correct cfg_set event

*For any* `bool` value `b`, calling `set_require_service_registration(b)` shall emit exactly one event with topic `cfg_set` and data `(svc_reg, b)`, and the stored flag shall equal `b`.

**Validates: Requirements 5.1, 5.2, 5.4**

### Property 6: `set_allowlist_enabled` always emits the correct cfg_set event

*For any* `bool` value `b`, calling `set_allowlist_enabled(b)` shall emit exactly one event with topic `cfg_set` and data `(allowlist, b)`, and the stored flag shall equal `b`.

**Validates: Requirements 6.1, 6.2, 6.4**

## Error Handling

No new error codes are introduced. Error handling for each setter is unchanged:

- `require_admin(&env)` panics with `EscrowError::NotInitialized` (#3) when called before `init`, or causes an auth failure (`Unauthorized`) when called by a non-admin. In both cases, neither the storage write nor the event publish is reached.
- The event `publish` call is placed unconditionally after the storage write. Because `env.events().publish` does not return a `Result` in the Soroban SDK, it cannot fail at the application level — the host either accepts or rejects the entire call frame. No try/catch or error branching is needed.

### Ordering invariant

For every affected setter:

```
1. require_admin      — panics early; no side effects if it panics
2. storage write      — persists the new value
3. events().publish   — emits the cfg_set event
```

This ordering matches the convention established by `set_service_price` and is the canonical pattern for admin mutations in this contract.

## Testing Strategy

### Dual testing approach

Both unit/example-based tests and property-based tests are used:

- **Property tests** verify that each setter emits the correct `cfg_set` event across all valid input values using [proptest](https://crates.io/crates/proptest) or the Soroban test harness's randomization support. Each property corresponds to one of the six correctness properties above. Minimum 100 iterations per property.
- **Example tests** verify specific error conditions (auth rejection) and non-regression of existing event payloads.

### Property tests

Each property test should:

1. Construct a fresh `Env` with `mock_all_auths()` and an initialized contract.
2. Call the setter with the generated input value.
3. Assert `env.events().all().last()` contains a topic `(cfg_set,)` and data decodable as the correct tuple `(key_symbol, value)`.
4. Assert the getter returns the same value.

Tag format: `Feature: config-change-events, Property N: <property_text>`

### Example tests

- Auth rejection: call each setter with auth stripped (`env.set_auths(&[])`), assert panic code #3, assert `env.events().all()` is empty or contains no `cfg_set` event.
- Non-regression: call `set_service_price`, `pause`, `unpause`, `settle` and assert their existing event payloads are unchanged by this diff.
- Boolean toggle: call `set_require_service_registration(true)` then `set_require_service_registration(false)` and assert both events are emitted with the correct value in each.
- Large numeric values: call `set_max_requests_per_call(u32::MAX)` and `set_rate_window_seconds(u64::MAX)` and assert correct encoding.

### Coverage

All six new `publish` call sites must be covered by at least one test each. The property tests over the full value domain provide substantially more than the 95% line-coverage target for the impacted module.
