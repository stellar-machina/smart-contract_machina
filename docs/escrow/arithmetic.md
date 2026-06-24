# Escrow arithmetic: overflow & saturation policy

This document records the deliberate integer-arithmetic strategy used by the
escrow contract (`contracts/escrow/src/lib.rs`). The policy is twofold:

1. **All accumulator and billing math saturates** — it never wraps and never
   panics the host on overflow.
2. **`overflow-checks = true` on the release profile** is a defense-in-depth
   backstop: any *non-saturating* `+`/`*` that gets added later will panic
   (trap) on overflow instead of silently wrapping. This is intentional —
   wrapping is the one outcome we never want on-chain.

## Why saturate instead of checked/panic?

Two distinct call contexts share the same answer:

- **Hot write path (`record_usage`).** This is the most frequently called
  entrypoint. Panicking here would block an agent from recording legitimate
  usage and could wedge the off-chain metering loop. The counters are designed
  to be drained by `settle` long before any realistic overflow horizon, so the
  safe failure mode is to clamp rather than to abort a state transition.
- **Read / settle path (`compute_billing`, `settle`).** These are read-mostly
  and are consumed by an off-chain settlement loop. Returning a clamped
  sentinel value lets that loop *detect* an anomaly and react, rather than
  having the host trap and the loop receive no signal at all.

## Saturating sites

| Site | Expression | Type | Rationale |
| --- | --- | --- | --- |
| `record_usage` — per-pair counter | `prev.saturating_add(requests)` | `u32` | saturate: settlement drains long before `u32::MAX`; never panic the hot path |
| `record_usage` — `TotalUsageByAgent` | `prev_total.saturating_add(requests)` | `u32` | same: lifetime per-agent counter, clamp not panic |
| `record_usage` — `TotalRequestsAllTime` | `proto_prev.saturating_add(requests as u64)` | `u64` | `u64` horizon; saturate not panic |
| `compute_billing` | `(requests as i128).saturating_mul(price)` | `i128` | saturate: read path returns a sentinel-large value rather than panicking; off-chain loop treats saturation as an error signal |
| `settle` | `(requests as i128).saturating_mul(price)` | `i128` | same math as `compute_billing`; settle path returns sentinel rather than panicking |

`price` is validated `>= 0` at `set_service_price`, and `requests` is a `u32`,
so the multiplicands are always non-negative and the only reachable boundary is
`i128::MAX`.

## What a saturated value MEANS to downstream consumers

A saturated value is **never** a silent wrap-around and **never** an ordinary
result. It is an out-of-band signal:

- A per-pair / per-agent counter pinned at `u32::MAX` means usage has been
  accumulating without ever being drained — i.e. the settlement loop is **stuck
  or not running** for that pair. Settlement resets the per-pair counter to
  zero, so a pinned counter is an accounting anomaly to investigate, not a bill
  to charge.
- A billing value equal to `i128::MAX` means `requests * price` overflowed
  `i128`. The off-chain settlement loop must treat this as an **error signal**
  (stuck settlement / mis-configured price), not as a real amount to transfer.

In all cases the contract has preserved a defined, monotonic, non-wrapping
value; correctness of the *response* to that value is the off-chain consumer's
responsibility.

## Defense-in-depth backstop

The release profile sets:

```toml
[profile.release]
overflow-checks = true
```

This does not affect the saturating operations above (they cannot overflow by
construction). Its purpose is forward-looking: if a future change introduces a
plain `+` or `*` on a path that *can* overflow, the release build will trap
rather than wrap. Wrapping arithmetic on accounting state is the single outcome
this policy rules out; saturation is the chosen graceful behaviour and the
overflow check is the safety net for everything else.
