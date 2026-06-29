# Dispute and Refund Flow

This document describes the dispute lifecycle for the `Escrow` contract, introduced to give agents recourse against over-reported usage records.

## Overview

When a billing discrepancy is detected, any authorized agent may open a dispute on a `(agent, service_id)` pair. While a dispute is open, `settle` is blocked for that pair. An admin must adjudicate by calling `resolve_dispute`, optionally crediting a refund before clearing the flag.

## State Diagram

```
[No dispute]
     │
     │ open_dispute(agent, service_id)
     ▼
[Dispute open]  ←── settle(…) → BLOCKED (DisputeOpen #18)
     │
     │ resolve_dispute(agent, service_id, refund_requests)  [admin only]
     ▼
[No dispute]  ←── settle(…) → allowed
```

### States

| State         | `has_open_dispute` | `settle` allowed |
|---------------|--------------------|------------------|
| No dispute    | `false`            | Yes              |
| Dispute open  | `true`             | No (#18)         |

## Entrypoints

### `open_dispute(agent, service_id)`

- **Auth:** `agent.require_auth()` — the disputing party must sign.
- **Pause gate:** Yes — panics `ContractPaused (#4)` while paused.
- **Precondition:** No open dispute exists for the pair (else `DisputeAlreadyOpen #19`).
- **Effect:** Persists `DataKey::Dispute(agent, service_id) = true`. Blocks `settle`.
- **Event:** `dispute("open", agent, service_id)`

### `resolve_dispute(agent, service_id, refund_requests)`

- **Auth:** Admin only (`admin.require_auth()`). Agents cannot self-resolve.
- **Pause gate:** Yes — panics `ContractPaused (#4)` while paused.
- **Preconditions:**
  - An open dispute exists for the pair (else `NoOpenDispute #20`).
  - `refund_requests <= current_usage` (else `RefundExceedsUsage #21`).
- **Effect:**
  - If `refund_requests > 0`: subtracts from `DataKey::Usage(agent, service_id)`.
  - Clears `DataKey::Dispute(agent, service_id)` (sets to `false`).
  - Unblocks `settle` for the pair.
- **Event:** `dispute("resolve", agent, service_id, refund_requests)`

### `has_open_dispute(agent, service_id) → bool`

Pure read. Returns `true` iff a dispute is currently open for the pair. No auth, no pause gate.

## Error Codes (append-only)

| Code | Name                  | Trigger                                        |
|------|-----------------------|------------------------------------------------|
| `18` | `DisputeOpen`         | `settle` called while a dispute is open        |
| `19` | `DisputeAlreadyOpen`  | `open_dispute` called when dispute already open |
| `20` | `NoOpenDispute`       | `resolve_dispute` called with no open dispute   |
| `21` | `RefundExceedsUsage`  | `refund_requests > current usage`              |

## Security Notes

- **No self-resolution:** `resolve_dispute` is gated by `admin.require_auth()`. The disputing agent cannot approve their own refund.
- **No double-refund:** `RefundExceedsUsage (#21)` prevents refunding more than the recorded usage. Usage is never allowed to go negative.
- **No dispute without record:** `open_dispute` works even on pairs with zero usage, but `resolve_dispute` with `refund_requests > 0` will panic `RefundExceedsUsage` since `0 > 0` is false — effectively a no-op refund is still possible via `refund_requests = 0`.
- **Single open dispute per pair:** Attempting to open a second dispute before resolving the first panics `DisputeAlreadyOpen (#19)`.
- **Pause gate:** Both `open_dispute` and `resolve_dispute` respect the emergency pause, consistent with all other state-changing entrypoints.
