# Escrow Pricing Model

This document describes the billing model supported by the `escrow` contract,
covering both the legacy flat-rate mode and the volume-discount tier mode
introduced in this update.

---

## Flat-rate billing (default)

When no tier schedule is configured for a service, billing uses a single
`price_per_request` value stored under `DataKey::ServicePrice(service_id)`.

```
bill = accumulated_requests * price_per_request   (saturating_mul, stroops)
```

Set via `set_service_price(service_id, price_stroops)`.  
Read via `get_service_price(service_id)`.

---

## Tiered volume-discount billing

An admin can attach a tier schedule to any service via `set_price_tiers`.
When a schedule is present, `compute_billing` and `settle` ignore
`ServicePrice` and apply the tier math instead.

### Tier schedule shape

A tier schedule is a `Vec<PriceTier>` where each entry is:

| field                | type   | meaning                                                  |
|----------------------|--------|----------------------------------------------------------|
| `threshold_requests` | `u32`  | Inclusive upper bound on cumulative requests in this tier |
| `price_stroops`      | `i128` | Marginal price per request within this tier (≥ 0)        |

### Schedule invariants (enforced at write-time by `set_price_tiers`)

1. The schedule must contain **at least one** entry.
2. `threshold_requests` values must be **strictly ascending** — no ties.
3. The first tier's `threshold_requests` must be **> 0**.
4. Every `price_stroops` must be **≥ 0**.

Violations are rejected with `EscrowError::InvalidPriceTiers` (#18).

### Tier-boundary semantics

Tiers are **inclusive** at the upper boundary:

- Tier 0 covers requests `[1 .. threshold_0]` (both endpoints included).
- Tier k covers requests `[threshold_{k-1}+1 .. threshold_k]`.
- The **last tier is open-ended**: any requests beyond the last threshold
  are still billed at the last tier's `price_stroops`.

A `threshold_requests` of `u32::MAX` on the final tier therefore means
"unlimited" and is the conventional choice when you want the last tier to
have no upper ceiling.

### Worked examples

#### Example 1 — three-tier schedule

```
tier 0: threshold=100,  price=10  stroops/request
tier 1: threshold=1000, price=7   stroops/request
tier 2: threshold=MAX,  price=4   stroops/request
```

| accumulated_requests | calculation                             | total (stroops) |
|----------------------|-----------------------------------------|-----------------|
| 50                   | 50 × 10                                 | 500             |
| 100                  | 100 × 10                                | 1 000           |
| 101                  | 100 × 10 + 1 × 7                        | 1 007           |
| 1 000                | 100 × 10 + 900 × 7                      | 7 300           |
| 1 001                | 100 × 10 + 900 × 7 + 1 × 4             | 7 304           |
| 5 000                | 100 × 10 + 900 × 7 + 4 000 × 4        | 23 300          |

#### Example 2 — single-tier schedule (uniform price for all usage)

```
tier 0: threshold=MAX, price=5 stroops/request
```

Equivalent to `set_service_price(svc, 5)` but using the tier path.

---

## API reference

### `set_price_tiers(service_id, tiers)`

Admin-gated. Stores the tier schedule under `DataKey::PriceTiers(service_id)`.
Validates the schedule (monotonicity, non-negative prices, non-empty) before
writing. Emits `tiers_set(service_id)`.

### `get_price_tiers(service_id) -> Option<Vec<PriceTier>>`

Pure read. Returns the stored schedule or `None` if the flat-rate path is in use.

### `remove_price_tiers(service_id)`

Admin-gated. Removes the tier schedule, reverting `compute_billing` and `settle`
to the flat `ServicePrice`. Idempotent. Emits `tiers_rm(service_id)`.

### `compute_billing(agent, service_id) -> i128`

Read-only. Returns the outstanding bill in stroops using whichever pricing
path is active (tier if set, flat otherwise). Saturates at `i128::MAX` rather
than panicking.

### `settle(caller, agent, service_id) -> i128`

Drains the usage counter and returns the billed amount using the same
tier-aware (or flat fallback) math as `compute_billing`.

---

## Security notes

- **Overflow safety**: all arithmetic is `saturating_mul` / `saturating_add`.
  A saturated return value (`i128::MAX`) is a sentinel for the off-chain
  settlement loop, not a panic.
- **Deterministic ordering**: the tier schedule is stored verbatim on-chain.
  `set_price_tiers` rejects non-monotonic input so the schedule read back by
  `compute_billing` and `settle` is always in ascending order; no re-sort is
  needed at read time.
- **No reentrancy surface**: `compute_billing` is read-only; `settle` drains
  the counter in a single persistent write before emitting the event.
- **Backward compatibility**: services without a tier schedule continue to use
  the flat `ServicePrice` path unchanged. Existing callers need no migration.
