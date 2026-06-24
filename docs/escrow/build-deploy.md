# Escrow: Build, Test, and Deploy Guide

End-to-end guide for building the `escrow` Soroban contract from source,
producing the release WASM, and deploying it to the Stellar testnet with the
Stellar/Soroban CLI.

> **Version-sensitive CLI flags are marked with ⚠️.** The Stellar CLI surface
> changes between releases (the tool was renamed from `soroban` to `stellar`,
> and several flags were renamed). Always confirm against
> `stellar contract deploy --help` for your installed version.

## Workspace layout

This is a Cargo workspace. The root `Cargo.toml` declares a single member:

```toml
[workspace]
resolver = "2"
members = ["contracts/escrow"]
```

All `cargo` commands below are run from the workspace root and operate on that
member. The contract crate (`contracts/escrow/Cargo.toml`) builds a
`cdylib` + `rlib` and depends on `soroban-sdk = "22.0"`.

## 1. Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain, with `rustfmt`).
- The `wasm32-unknown-unknown` target, required to compile to WASM:

  ```bash
  rustup target add wasm32-unknown-unknown
  ```

- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli)
  (formerly the Soroban CLI), for deploying and invoking. Install with:

  ```bash
  cargo install --locked stellar-cli
  ```

## 2. Run the test suite

From the workspace root, run the native (host) tests for every member:

```bash
cargo fmt --all -- --check
cargo build
cargo test
```

`cargo test` compiles the contract with the `testutils` feature and runs the
unit tests in `contracts/escrow/src/test.rs`. All three commands must pass
before you build a release artifact.

## 3. Build the release WASM

Build the optimized WASM for the `wasm32-unknown-unknown` target:

```bash
cargo build --release --target wasm32-unknown-unknown
```

The release profile in the root `Cargo.toml` is tuned for the smallest,
most predictable on-chain footprint:

```toml
[profile.release]
opt-level = "z"        # optimize aggressively for size
overflow-checks = true # keep arithmetic overflow checks in release
debug = false
lto = true             # link-time optimization across the crate graph
codegen-units = 1      # maximize cross-function optimization
panic = "abort"        # no unwinding machinery in the WASM
strip = "symbols"      # drop symbol tables from the artifact
```

`opt-level = "z"`, `lto = true`, and `panic = "abort"` together keep the WASM
small; `overflow-checks = true` is deliberately retained so the contract traps
on arithmetic overflow rather than wrapping silently.

### Where the artifact lands

The compiled module is written to:

```
target/wasm32-unknown-unknown/release/escrow.wasm
```

> **Note:** the workspace shares a single `target/` directory, so the artifact
> path is relative to the workspace root, not the crate directory.

### (Optional) further-optimize the WASM

The Stellar CLI can shrink the artifact further before deployment:

```bash
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm
```

This writes `escrow.optimized.wasm` alongside the input.

## 4. Configure a testnet identity

⚠️ Identity/key management flags vary by CLI version. The current form:

```bash
# Generate and fund a key on testnet
stellar keys generate --global deployer --network testnet --fund

# Inspect the public address
stellar keys address deployer
```

## 5. Deploy to testnet

⚠️ `--network`, `--source-account`, and `--wasm` are the current flag names.
Older `soroban` releases used `--source` instead of `--source-account`.

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source-account deployer \
  --network testnet
```

On success the CLI prints the deployed **contract ID** (a `C...` strkey).
Export it for the subsequent calls:

```bash
export CONTRACT_ID=<printed-contract-id>
```

## 6. Initialize the contract

The contract stores its operational admin at `init` and rejects a second call
with `AlreadyInitialized` (error code `#1`). Invoke `init(admin)` with the
deployer as admin:

⚠️ Contract arguments come after the `--` separator; argument-naming follows the
function signature (`admin`).

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account deployer \
  --network testnet \
  -- \
  init --admin "$(stellar keys address deployer)"
```

## 7. Post-deploy sanity check

Confirm the deployment responds and reports the expected versions.

`version()` returns the compiled contract version (currently `2`):

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account deployer \
  --network testnet \
  -- \
  version
```

`get_schema_version()` returns the on-chain **storage schema** version
(defaults to `1` on a fresh deploy, before any `migrate_v1_to_v2`):

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account deployer \
  --network testnet \
  -- \
  get_schema_version
```

You can also confirm the admin was stored:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account deployer \
  --network testnet \
  -- \
  get_admin
```

A non-error response from `version` (and an admin matching the deployer) means
the contract is live and initialized.

## Summary checklist

- [ ] `rustup target add wasm32-unknown-unknown`
- [ ] `cargo fmt --all -- --check && cargo build && cargo test`
- [ ] `cargo build --release --target wasm32-unknown-unknown`
- [ ] artifact at `target/wasm32-unknown-unknown/release/escrow.wasm`
- [ ] `stellar contract deploy ...` → capture `CONTRACT_ID`
- [ ] `init --admin <deployer>`
- [ ] `version` / `get_schema_version` sanity check
