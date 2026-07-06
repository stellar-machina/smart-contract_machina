# Stellar Machina — On-Chain Escrow

[![CI](https://github.com/stellar-machina/smart-contract_machina/actions/workflows/ci.yml/badge.svg)](https://github.com/stellar-machina/smart-contract_machina/actions/workflows/ci.yml)

The **Soroban** smart contracts that give Stellar Machina its trust anchor on
Stellar. Agents pay per request; usage is tallied off-chain for speed, and this
`escrow` contract is where that usage becomes money — recording accumulators,
computing charges from a service's pricing, and settling balances on-chain.

If the [backend](https://github.com/stellar-machina/backend_machina) is the meter,
this is the ledger it answers to.

---

## The settlement model

Stellar Machina separates *counting* from *paying*, because counting every
request on-chain would be slow and expensive:

1. **Register** — a service is registered on-chain with its price (flat or tiered).
2. **Record** — as agents consume the service, `record_usage` bumps per-agent,
   per-service counters held in contract storage.
3. **Settle** — `settle` / `settle_all` computes the bill from the recorded usage
   and the service's pricing, drains the counters, and finalizes the transfer.
4. **Adjudicate** — if something is contested, the dispute flow
   (`open_dispute` → `resolve_dispute`) gates settlement until it's resolved.

Every state change worth watching emits a decodable event (for example `cfg_set`
for policy changes, plus events around draining and disputes), so an off-chain
indexer can reconstruct the full history without trusting the caller.

---

## What the escrow contract governs

The public interface (in `contracts/escrow/src/lib.rs`) clusters into a few
responsibilities:

- **Service lifecycle** — `register_service`, `register_service_with_metadata`,
  `set_service_metadata`, `set_service_disabled`, `transfer_service_ownership`,
  `unregister_service`.
- **Pricing** — `set_service_price`, `set_price_tiers`, `remove_price_tiers`,
  `compute_billing`, and the matching getters.
- **Usage & settlement** — `record_usage`, `decrement_usage`, `settle`,
  `settle_all`, `get_last_settlement`, and paged/batch usage reads.
- **Access policy** — an allowlist (`set_agent_allowed`, `set_agent_blocked`,
  `set_allowlist_enabled`) and rate limits
  (`set_max_requests_per_call`, `set_max_requests_per_window`,
  `set_rate_window_seconds`).
- **Disputes** — `open_dispute`, `resolve_dispute`, `has_open_dispute`.
- **Administration** — a two-step admin handover
  (`propose_admin_transfer` → `accept_admin_transfer` / `cancel_admin_transfer`)
  and an emergency `pause` / `unpause`.
- **Versioning** — `get_schema_version` and `migrate_v` for controlled upgrades.

---

## Build, test, and package

**Prerequisites:** a stable Rust toolchain with the `wasm32-unknown-unknown`
target. The pinned toolchain (including `rustfmt` and `clippy`) is declared in
[`rust-toolchain.toml`](./rust-toolchain.toml), so `rustup` will provision it for
you automatically.

```bash
# Fast feedback: run the contract test suite natively
cargo test

# Formatting and lint gates
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Produce the optimized on-chain wasm
cargo build --release --target wasm32-unknown-unknown
```

The release profile in [`Cargo.toml`](./Cargo.toml) is tuned for small, safe wasm:
`opt-level = "z"`, fat LTO, a single codegen unit, `panic = "abort"`, stripped
symbols, and `overflow-checks = true` (arithmetic safety is kept on even in
release — see [`docs/escrow/arithmetic.md`](./docs/escrow/arithmetic.md)).

---

## Deep-dive documentation

The `docs/escrow/` folder is the authoritative reference for auditors and
integrators:

| Document | Covers |
| --- | --- |
| [`api.md`](./docs/escrow/api.md) | Full function-by-function reference |
| [`pricing.md`](./docs/escrow/pricing.md) | Flat and tiered pricing model |
| [`disputes.md`](./docs/escrow/disputes.md) | Dispute lifecycle and settlement gating |
| [`storage.md`](./docs/escrow/storage.md) | On-chain storage layout |
| [`arithmetic.md`](./docs/escrow/arithmetic.md) | Overflow safety and rounding |
| [`validation-order.md`](./docs/escrow/validation-order.md) | Guard/check ordering guarantees |
| [`errors.md`](./docs/escrow/errors.md) | Error codes and their meanings |
| [`security.md`](./docs/escrow/security.md) | Authorization model |
| [`migrations.md`](./docs/escrow/migrations.md) | Schema versioning and upgrades |
| [`build-deploy.md`](./docs/escrow/build-deploy.md) | Building and deploying the wasm |

---

## Workspace layout

```
.
├── Cargo.toml              # workspace + release profile
├── rust-toolchain.toml     # pinned toolchain + wasm target
├── contracts/
│   └── escrow/             # the escrow contract crate (lib.rs, test.rs)
└── docs/escrow/            # reference documentation
```

See [`CHANGELOG.md`](./CHANGELOG.md) for release history and
[`CONTRIBUTING.md`](./CONTRIBUTING.md) before submitting changes.

## License

Released under the [MIT License](./LICENSE).
