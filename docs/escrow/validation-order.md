# `record_usage` Validation Precedence

`record_usage` enforces a **fixed, stable validation order**. The order is part of
the public contract: client SDKs and settlement loops that pattern-match on error
codes depend on which error fires first when multiple conditions are simultaneously
true. Changing the order is a breaking change and requires a major version bump.

## Why ordering matters

When two or more guard conditions are violated at once, only the first error is
returned. Integrators that inspect error codes to decide whether to retry, alert,
or skip a record must be able to predict which code fires. Fixing this order means
the behaviour is deterministic and testable: "paused always beats zero-requests"
is a property the test suite can assert, and clients can rely on it forever.

## Precedence table

| # | Gate | Error raised | Code | Conditional? | Rationale |
|---|------|--------------|------|--------------|-----------|
| 1 | Contract paused | `ContractPaused` | `#4` | No — always checked | Emergency-stop; must be the very first gate so no other logic runs during an incident. |
| 2 | `requests == 0` | `RequestsMustBePositive` | `#2` | No — always checked | Structural argument validation comes before any storage read to avoid burning I/O on a trivially invalid call. |
| 3 | `requests > MaxRequestsPerCall` | `RequestsExceedsMaxPerCall` | `#8` | No — cap defaults to `u32::MAX` (inactive) | Per-call bounds are cheap scalar comparisons; checked before service/agent lookups to fail fast. |
| 4 | `requests < MinRequestsPerCall` | `RequestsBelowMinPerCall` | `#9` | No — floor defaults to `0` (inactive) | Paired with the max cap; both scalar, both before service/agent I/O. |
| 5 | Service not registered | `ServiceNotRegistered` | `#7` | Yes — only when `RequireServiceRegistration` flag is `true` | Service existence is a prerequisite for all agent-level checks; checked before agent flags. `ServiceRegistered` read is short-circuited when the flag is off. |
| 6 | Service disabled | `ServiceDisabled` | `#12` | No — always checked | A disabled service is rejected regardless of agent status. Placed after registration so unknown services get `#7`, not `#12`. |
| 7 | Agent on blocklist | `AgentBlocked` | `#17` | No — unconditional read | Blocklist is an explicit hard-stop; it overrides the allowlist. Checked before the allowlist so a blocked-but-allowlisted agent is always rejected. |
| 8 | Agent not on allowlist | `AgentNotAllowed` | `#10` | Yes — only when `AllowlistEnabled` flag is `true` | Permissive by default; opt-in restriction. `AgentAllowed` read is short-circuited when the flag is off. |
| 9 | Rate limit exceeded | `RateLimitExceeded` | `#15` | Yes — only when both `MaxRequestsPerWindow > 0` and `WindowSeconds > 0` | Involves a mutable per-agent state update (`RateWindow`); placed last among validations so the window counter is only advanced when all other checks have passed. Disabled by default (`0` values). |

## Conditional reads

Several checks are gated behind a controlling flag to avoid unnecessary storage
I/O when a feature is not in use:

- `ServiceRegistered(service_id)` — read only when `RequireServiceRegistration` is `true`.
- `AgentAllowed(agent)` — read only when `AllowlistEnabled` is `true`.
- `RateWindow(agent)` — read (and written) only when both `MaxRequestsPerWindow > 0`
  and `WindowSeconds > 0`.

Unconditionally-read keys in every call: `Paused`, `MaxRequestsPerCall`,
`MinRequestsPerCall`, `ServiceDisabled(service_id)`, `AgentBlocked(agent)`.

## Reconciling the inline comment

The inline precedence comment in `record_usage` (in `contracts/escrow/src/lib.rs`)
previously listed only 7 gates (items 1–7 above), omitting the blocklist
(`AgentBlocked #17`, now item 7) and rate-limit (`RateLimitExceeded #15`, now
item 9) checks that were added later. The comment has been updated to reflect all
9 gates in their implemented order.

## Stability guarantee

This table is authoritative. The ordering is tested in the contract test suite
(see `contracts/escrow/src/test.rs`). Any future addition to the validation chain
must:

1. Insert the new gate at a documented, justified position.
2. Update this table.
3. Update the inline comment in `record_usage`.
4. Add a precedence test confirming the new gate's relationship to its neighbours.
5. Note the change in `CHANGELOG.md` as a **breaking change** if any existing
   gate shifts position.
