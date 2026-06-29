# Implementation Plan: Config Change Events

## Overview

Add a `cfg_set` event emission to each of the six silent config setters in `contracts/escrow/src/lib.rs`. The change is purely additive: a single `env.events().publish(...)` line is appended to each setter body, after the existing storage write, with no reordering of any existing logic. Tests are added to `contracts/escrow/src/test.rs` and documentation is added to `README.md`.

## Tasks

- [x] 1. Add `cfg_set` events to the four numeric config setters
  - [x] 1.1 Add `cfg_set` event to `set_max_requests_per_call`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("max_call"), max_requests));` after the existing `env.storage().persistent().set(...)` line in `set_max_requests_per_call`
    - Add a `///` doc comment noting the event emission, matching existing NatSpec style in the file
    - _Requirements: 1.1, 1.4, 7.1, 7.2, 7.3_

  - [ ]* 1.2 Write property test for `set_max_requests_per_call` event
    - **Property 1: `set_max_requests_per_call` always emits the correct cfg_set event**
    - Generate representative u32 values (0, 1, u32::MAX, and at least one midrange value) and assert `env.events().all().last()` decodes as `(symbol_short!("cfg_set"),)` topic and `(symbol_short!("max_call"), value): (Symbol, u32)` data
    - Also assert `client.get_max_requests_per_call()` returns the same value (round-trip)
    - **Validates: Requirements 1.1, 1.2, 1.4**

  - [x] 1.3 Add `cfg_set` event to `set_min_requests_per_call`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("min_call"), min_requests));` after the existing storage write in `set_min_requests_per_call`
    - Add `///` doc comment
    - _Requirements: 2.1, 2.4, 7.1, 7.2, 7.3_

  - [ ]* 1.4 Write property test for `set_min_requests_per_call` event
    - **Property 2: `set_min_requests_per_call` always emits the correct cfg_set event**
    - Test with 0, 1, u32::MAX and a midrange value; assert topic, key `min_call`, and value type `u32`; assert getter round-trip
    - **Validates: Requirements 2.1, 2.2, 2.4**

  - [x] 1.5 Add `cfg_set` event to `set_max_requests_per_window`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("max_win"), max_requests));` after the existing storage write in `set_max_requests_per_window`
    - Add `///` doc comment
    - _Requirements: 3.1, 3.3, 7.1, 7.2, 7.3_

  - [ ]* 1.6 Write property test for `set_max_requests_per_window` event
    - **Property 3: `set_max_requests_per_window` always emits the correct cfg_set event**
    - Test with 0 (limiter-disabled case), 1, u32::MAX, and a midrange value; assert topic, key `max_win`, value type `u32`; assert getter round-trip
    - **Validates: Requirements 3.1, 3.3**

  - [x] 1.7 Add `cfg_set` event to `set_rate_window_seconds`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("win_sec"), window_seconds));` after the existing storage write in `set_rate_window_seconds`
    - Add `///` doc comment
    - _Requirements: 4.1, 4.4, 7.1, 7.2, 7.3_

  - [ ]* 1.8 Write property test for `set_rate_window_seconds` event
    - **Property 4: `set_rate_window_seconds` always emits the correct cfg_set event**
    - Test with 0 (disabled case), 1, u64::MAX, and a midrange u64 value; assert topic, key `win_sec`, value type `u64`; assert getter round-trip
    - **Validates: Requirements 4.1, 4.2, 4.4**

- [-] 2. Checkpoint — build and test numeric setters
  - Run `cargo build` and `cargo test` to confirm the four new event emissions compile and the property tests pass. Fix any type-mismatch or macro errors before proceeding.

- [ ] 3. Add `cfg_set` events to the two boolean config setters
  - [~] 3.1 Add `cfg_set` event to `set_require_service_registration`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("svc_reg"), required));` after the `write_flag(...)` call in `set_require_service_registration`
    - Add `///` doc comment
    - _Requirements: 5.1, 5.2, 5.4, 7.1, 7.2, 7.3_

  - [ ]* 3.2 Write property test for `set_require_service_registration` event
    - **Property 5: `set_require_service_registration` always emits the correct cfg_set event**
    - Test with both `true` and `false`; assert topic `cfg_set`, key `svc_reg`, value type `bool` in each case; assert getter round-trip (`is_service_registration_required`)
    - **Validates: Requirements 5.1, 5.2, 5.4**

  - [~] 3.3 Add `cfg_set` event to `set_allowlist_enabled`
    - Append `env.events().publish((symbol_short!("cfg_set"),), (symbol_short!("allowlist"), enabled));` after the `write_flag(...)` call in `set_allowlist_enabled`
    - Add `///` doc comment
    - _Requirements: 6.1, 6.2, 6.4, 7.1, 7.2, 7.3_

  - [ ]* 3.4 Write property test for `set_allowlist_enabled` event
    - **Property 6: `set_allowlist_enabled` always emits the correct cfg_set event**
    - Test with both `true` and `false`; assert topic `cfg_set`, key `allowlist`, value type `bool` in each case; assert getter round-trip (`is_allowlist_enabled`)
    - **Validates: Requirements 6.1, 6.2, 6.4**

- [ ] 4. Write example-based tests for error paths and edge cases
  - [~] 4.1 Write auth-rejection tests for all six setters
    - For each setter, write a `#[should_panic(expected = "Error(Contract, #3)")]` test that calls the setter with `env.set_auths(&[])` and asserts the panic. Follow the pattern of `test_decrement_usage_rejects_non_admin` in the existing test suite.
    - _Requirements: 1.3, 2.3, 3.2, 4.2, 5.3, 6.3_

  - [ ]* 4.2 Write edge-case tests for boundary and toggle values
    - `set_max_requests_per_call(u32::MAX)` — assert event data value equals `u32::MAX`
    - `set_rate_window_seconds(u64::MAX)` — assert event data value equals `u64::MAX`
    - `set_require_service_registration(true)` followed by `set_require_service_registration(false)` — assert both events emitted with correct bool value each time
    - `set_allowlist_enabled(false)` when already `false` — assert event still emitted (idempotent-emit requirement)
    - _Requirements: 1.2, 2.2, 5.1, 5.2, 6.1, 6.2_

  - [ ]* 4.3 Write non-regression tests for existing events
    - Call `set_service_price`, `pause`, `unpause`, and `settle` after this patch is applied; assert their event topics and payloads are identical to pre-patch assertions already in the test suite
    - _Requirements: 7.4_

- [~] 5. Checkpoint — full test suite
  - Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`. All new and existing tests must pass. Fix any formatting or compilation issues.

- [ ] 6. Document the config-event catalogue in README.md
  - [~] 6.1 Add a "Config-change events (`cfg_set`)" section to `README.md`
    - Add the section after the existing "Per-agent rate limiting" section
    - The section must include:
      - A description of the unified `cfg_set` topic
      - A table listing each config key, value type, triggering setter, and default value when absent
      - A note that all six events share the `cfg_set` topic and the config key in the data tuple position 0 identifies the field
      - A brief subscriber decoding example (pseudocode)
    - _Requirements: 9.1, 9.2, 9.3_

- [~] 7. Final checkpoint — ensure all tests pass
  - Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`. Ensure all tests pass. Ask the user if any questions arise before declaring the feature complete.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP; core correctness is covered by mandatory tasks.
- All changes are in `contracts/escrow/src/lib.rs` (event emission) and `contracts/escrow/src/test.rs` (tests), plus an additive section in `README.md`.
- Do not reorder any existing logic in any setter; the `publish` call is always the last statement in each setter body.
- The `symbol_short!` macro enforces the ≤ 9 character constraint at compile time — all six config keys satisfy this.
- Each property test must reference the design document property by number in a comment, e.g. `// Feature: config-change-events, Property 1: set_max_requests_per_call always emits the correct cfg_set event`.

## Task Dependency Graph

```json
{
  "waves": [
    { "wave": 1, "tasks": ["1"] },
    { "wave": 2, "tasks": ["2"] },
    { "wave": 3, "tasks": ["3"] },
    { "wave": 4, "tasks": ["4"] },
    { "wave": 5, "tasks": ["5"] },
    { "wave": 6, "tasks": ["6"] },
    { "wave": 7, "tasks": ["7"] }
  ]
}
```
