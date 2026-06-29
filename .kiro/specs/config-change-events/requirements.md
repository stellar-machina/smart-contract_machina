# Requirements Document

## Introduction

The AgentPay escrow contract exposes several administrative setters that mutate operational policy — `set_max_requests_per_call`, `set_min_requests_per_call`, `set_max_requests_per_window`, `set_rate_window_seconds`, `set_require_service_registration`, and `set_allowlist_enabled` — but none of them currently emit an on-chain event. Unlike `set_service_price` (which emits `price_set`) or `pause`/`unpause` (which emit `paused`), an indexer or security monitor has no on-chain signal when an operator tightens or loosens these limits. This feature adds a consistent, decodable `cfg_set` event to each setter, closing the observability gap for all policy mutations.

## Glossary

- **Escrow**: The Soroban smart contract at `contracts/escrow/src/lib.rs` that records agent usage and manages settlement policy.
- **Config setter**: Any admin-gated entrypoint that writes a global operational-policy value (`set_max_requests_per_call`, `set_min_requests_per_call`, `set_max_requests_per_window`, `set_rate_window_seconds`, `set_require_service_registration`, `set_allowlist_enabled`).
- **Config event**: A Soroban event emitted by a config setter with a unified `cfg_set` topic and a typed data tuple.
- **cfg_set**: The `symbol_short!` topic string used for all config-change events.
- **Config key**: A `symbol_short!` string (≤ 9 chars) that identifies which configuration field changed (e.g. `max_call`, `min_call`, `max_win`, `win_sec`, `svc_reg`, `allowlist`).
- **Admin**: The privileged operator address stored at `DataKey::Admin`, set during `init`.
- **Subscriber**: Any off-chain indexer, security monitor, or event consumer that decodes Soroban contract events.
- **symbol_short!**: The Soroban SDK macro for creating short symbols (≤ 9 bytes); required for event topics.

## Requirements

### Requirement 1: Emit config-change event from `set_max_requests_per_call`

**User Story:** As a security monitor or indexer, I want to observe when the per-call request cap changes, so that I can detect tightening or loosening of call-level rate limits in real time.

#### Acceptance Criteria

1. WHEN `set_max_requests_per_call` is called with any `u32` value, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("max_call"), value)` after writing the new value to storage.
2. WHEN `set_max_requests_per_call` is called with the same value already stored, THE Escrow SHALL still publish the `cfg_set` event (unconditional emission on every successful call).
3. IF `set_max_requests_per_call` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
4. THE `cfg_set` event for `set_max_requests_per_call` SHALL use the config key `symbol_short!("max_call")` and SHALL encode the value as `u32`.

### Requirement 2: Emit config-change event from `set_min_requests_per_call`

**User Story:** As a security monitor or indexer, I want to observe when the per-call request floor changes, so that I can track whether minimum batch sizes are being enforced or relaxed.

#### Acceptance Criteria

1. WHEN `set_min_requests_per_call` is called with any `u32` value, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("min_call"), value)` after writing the new value to storage.
2. WHEN `set_min_requests_per_call` is called with the same value already stored, THE Escrow SHALL still publish the `cfg_set` event (unconditional emission on every successful call).
3. IF `set_min_requests_per_call` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
4. THE `cfg_set` event for `set_min_requests_per_call` SHALL use the config key `symbol_short!("min_call")` and SHALL encode the value as `u32`.

### Requirement 3: Emit config-change event from `set_max_requests_per_window`

**User Story:** As a security monitor or indexer, I want to observe when the per-window request cap changes, so that I can detect when rate-limiting policy is updated.

#### Acceptance Criteria

1. WHEN `set_max_requests_per_window` is called with any `u32` value, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("max_win"), value)` after writing the new value to storage.
2. IF `set_max_requests_per_window` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
3. THE `cfg_set` event for `set_max_requests_per_window` SHALL use the config key `symbol_short!("max_win")` and SHALL encode the value as `u32`.

### Requirement 4: Emit config-change event from `set_rate_window_seconds`

**User Story:** As a security monitor or indexer, I want to observe when the rate-limit window duration changes, so that I can track the temporal scope of agent-level rate limiting.

#### Acceptance Criteria

1. WHEN `set_rate_window_seconds` is called with any `u64` value, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("win_sec"), value)` after writing the new value to storage.
2. WHEN `set_rate_window_seconds` is called with `0` (disabling the window), THE Escrow SHALL still publish the `cfg_set` event with the value `0u64`.
3. IF `set_rate_window_seconds` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
4. THE `cfg_set` event for `set_rate_window_seconds` SHALL use the config key `symbol_short!("win_sec")` and SHALL encode the value as `u64`.

### Requirement 5: Emit config-change event from `set_require_service_registration`

**User Story:** As a security monitor or indexer, I want to observe when the strict-registration mode toggles, so that I can detect when the contract switches between open and gated service registration.

#### Acceptance Criteria

1. WHEN `set_require_service_registration` is called with `true`, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("svc_reg"), true)` after writing the flag to storage.
2. WHEN `set_require_service_registration` is called with `false`, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("svc_reg"), false)` after writing the flag to storage.
3. IF `set_require_service_registration` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
4. THE `cfg_set` event for `set_require_service_registration` SHALL use the config key `symbol_short!("svc_reg")` and SHALL encode the value as `bool`.

### Requirement 6: Emit config-change event from `set_allowlist_enabled`

**User Story:** As a security monitor or indexer, I want to observe when the allowlist gate toggles, so that I can detect when agent admission policy changes between open and restricted access.

#### Acceptance Criteria

1. WHEN `set_allowlist_enabled` is called with `true`, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("allowlist"), true)` after writing the flag to storage.
2. WHEN `set_allowlist_enabled` is called with `false`, THE Escrow SHALL publish an event with topic `cfg_set` and data tuple `(symbol_short!("allowlist"), false)` after writing the flag to storage.
3. IF `set_allowlist_enabled` is called by a non-admin, THEN THE Escrow SHALL check admin permissions first, reject the call immediately with `EscrowError::NotInitialized` (code #3), and SHALL NOT emit any event.
4. THE `cfg_set` event for `set_allowlist_enabled` SHALL use the config key `symbol_short!("allowlist")` and SHALL encode the value as `bool`.

### Requirement 7: Unified, decodable config-event schema

**User Story:** As a subscriber building an indexer or monitoring tool, I want all config-change events to follow a single schema, so that I can decode every config mutation with one handler.

#### Acceptance Criteria

1. THE Escrow SHALL use the single topic `symbol_short!("cfg_set")` for all six config setters listed in Requirements 1–6.
2. THE Escrow SHALL encode every config-change event data as a two-element tuple `(Symbol, T)` where the first element is the config key (a `symbol_short!` string ≤ 9 bytes) and the second element is the new value.
3. THE config keys used by config-change events SHALL be: `max_call` (u32), `min_call` (u32), `max_win` (u32), `win_sec` (u64), `svc_reg` (bool), `allowlist` (bool).
4. THE Escrow SHALL NOT change the payload format or topic of existing events (`price_set`, `paused`, `settled`, `usage`, `usage_dec`, `usage_hi`, `price_rm`, `tiers_set`, `tiers_rm`, `owner_chg`, `meta_clr`).
5. THE Escrow SHALL emit config-change events only after the corresponding storage write succeeds and only on successful (non-panicking) invocations.

### Requirement 8: Security and information-disclosure invariant

**User Story:** As a protocol security reviewer, I want to confirm that config-change events disclose no more information than the current readable contract state, so that emitting these events does not introduce a new information-disclosure surface.

#### Acceptance Criteria

1. THE config-change event payload for each setter SHALL contain only the config key and the new value — the same data already readable via the corresponding getter entrypoint.
2. THE config key symbols used in config-change events SHALL each be at most 9 characters long, satisfying the Soroban `symbol_short!` macro constraint.
3. THE Escrow SHALL NOT include the caller address, previous value, or any other data beyond the config key and new value in a config-change event payload.

### Requirement 9: Documentation of the config-event catalogue

**User Story:** As a developer integrating with AgentPay, I want the full config-event catalogue documented in the README and/or the existing API docs, so that I can discover event topics and schemas without reading the contract source.

#### Acceptance Criteria

1. THE README.md SHALL contain a section documenting all config-change events, listing each event topic, config key, value type, and the setter entrypoint that emits it.
2. THE config-event documentation SHALL note that a single `cfg_set` topic covers all six setters and explain how to distinguish events by the config key in the data tuple.
3. THE config-event documentation SHALL be additive — existing documentation sections SHALL NOT be removed or structurally reorganised.
