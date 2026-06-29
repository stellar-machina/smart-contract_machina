#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, String, Symbol, Vec,
};

/// Current on-chain storage schema version stamped at init.
const CURRENT_SCHEMA: u32 = 2;

/// Maximum number of `(agent, service_id)` pairs accepted by a single
/// `get_usage_batch` call. Chosen at 100 as a conservative cap: the batch
/// read iterates the input once doing one persistent read per pair, so the
/// bound keeps the loop (and the host's storage-read budget) predictable and
/// prevents a single call from triggering an unboundedly large amount of work.
/// Callers needing more pairs should page the requests.
pub const MAX_BATCH_READ: u32 = 100;

/// Maximum number of service ids accepted by a single
/// `get_services_status_batch` call. Mirrors `MAX_BATCH_READ` — the batch
/// read iterates the input once doing three persistent reads per service id
/// (registered flag, disabled flag, price), so the bound keeps the loop and
/// the host's storage-read budget predictable. Callers needing more ids should
/// page the requests.
pub const MAX_SERVICE_STATUS_BATCH: u32 = 100;

/// Per-service status snapshot returned by [`Escrow::get_services_status_batch`].
///
/// Each field maps 1-to-1 to the corresponding single-service getter so the
/// batched and individual read paths cannot drift:
/// - `registered` mirrors `is_service_registered`
/// - `disabled`   mirrors `is_service_disabled`
/// - `price_stroops` mirrors `get_service_price`
///
/// Unknown services (never registered/disabled/priced) produce an entry with
/// `registered = false`, `disabled = false`, and `price_stroops = 0`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceStatus {
    pub service_id: Symbol,
    pub registered: bool,
    pub disabled: bool,
    pub price_stroops: i128,
}

/// Free-form metadata about a service. Stored under
/// `DataKey::ServiceMetadata(service_id)` so dashboards and clients can
/// resolve a service to a human-readable description and owner without
/// keeping a parallel registry off-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceMetadata {
    pub description: String,
    pub owner: Address,
}

/// Storage keys used by the escrow contract.
///
/// Persistent slots survive across full TTL cycles and are appropriate for
/// long-lived configuration (e.g. the admin address) and for per-(agent,
/// service) usage accumulators that AgentPay's settlement loop reads.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Operational admin address; set once at `init`.
    Admin,
    /// Accumulated usage counter for a given `(agent, service_id)` pair.
    Usage(Address, Symbol),
    /// Price per request, in stroops, for a registered service.
    ServicePrice(Symbol),
    /// `true` when the contract is paused (no state-changing entrypoints
    /// accept calls).
    Paused,
    /// Pending admin address proposed via `propose_admin_transfer`,
    /// waiting on `accept_admin_transfer`. Two-step handover prevents
    /// accidentally locking out of the contract via a bad signing key.
    PendingAdmin,
    /// `true` if a service is registered (i.e. admin has explicitly
    /// listed it). When `RequireServiceRegistration` is enabled,
    /// `record_usage` rejects unknown services with a typed error.
    ServiceRegistered(Symbol),
    /// `true` when `record_usage` should reject unknown services.
    RequireServiceRegistration,
    /// Upper bound on `requests` per single `record_usage` call. When
    /// set, `record_usage` rejects calls above this delta. Defaults to
    /// `u32::MAX` (no limit) when absent.
    MaxRequestsPerCall,
    /// Lower bound on `requests` per single `record_usage` call.
    /// Useful for amortising the per-write ledger cost.
    MinRequestsPerCall,
    /// Per-agent allowlist flag. When `AllowlistEnabled` is true,
    /// `record_usage` rejects agents whose entry is absent or false.
    AgentAllowed(Address),
    /// Master toggle: when true, the per-agent allowlist is enforced.
    AllowlistEnabled,
    /// Cross-service total request count for a given agent.
    /// Settlement does NOT reset this counter; it is the lifetime
    /// signal for analytics and SLA tiering.
    TotalUsageByAgent(Address),
    /// Protocol-wide lifetime request counter, written by every
    /// successful `record_usage`. Useful as a single grafana gauge.
    TotalRequestsAllTime,
    /// Ledger timestamp (seconds since unix epoch) at which the last
    /// `settle` call drained this `(agent, service_id)` pair. Lets
    /// off-chain SLA monitoring catch stuck settlement cycles.
    LastSettlement(Address, Symbol),
    /// On-chain storage schema version. Distinct from the contract
    /// version() (which is the compiled wasm version): SchemaVersion
    /// tracks what the persisted state layout looks like so callers can
    /// confirm a `migrate` has run on a redeployed contract.
    SchemaVersion,
    /// Free-form metadata (`description`, `owner`) about a service.
    ServiceMetadata(Symbol),
    /// `true` when a service has been temporarily disabled by admin.
    /// Distinct from `ServiceRegistered`: a registered service can be
    /// disabled without unregistering, preserving the metadata and the
    /// per-(agent, service) usage history.
    ServiceDisabled(Symbol),
    /// Max `requests` an agent may accumulate within one rate-limit
    /// window. `0` (the default) disables the limiter entirely.
    MaxRequestsPerWindow,
    /// Length of the fixed rate-limit window in seconds. `0` (the
    /// default) disables the limiter entirely.
    WindowSeconds,
    /// Per-agent fixed-window rate-limit state: `(window_start, count)`
    /// where `window_start` is the ledger timestamp the current window
    /// opened and `count` is the requests accumulated in it so far.
    RateWindow(Address),
    /// Per-agent blocklist flag. When `true`, `record_usage` rejects the
    /// agent with `AgentBlocked`, taking precedence over the allowlist.
    AgentBlocked(Address),
}

/// Typed contract errors. Codes are append-only to keep client SDKs stable.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    /// `init` was already called and the admin address is already stored.
    AlreadyInitialized = 1,
    /// `record_usage` was called with `requests == 0`.
    RequestsMustBePositive = 2,
    /// An admin-gated entrypoint was invoked but the admin is not set.
    NotInitialized = 3,
    /// A state-changing entrypoint was called while `Paused` is `true`.
    ContractPaused = 4,
    /// `accept_admin_transfer` was called but no pending admin is set.
    NoPendingAdminTransfer = 5,
    /// `accept_admin_transfer` was called by the wrong address.
    NotPendingAdmin = 6,
    /// `record_usage` referenced a service that has not been registered
    /// while strict registration is enabled.
    ServiceNotRegistered = 7,
    /// `record_usage` was called with a `requests` value above the
    /// configured `MaxRequestsPerCall` cap.
    RequestsExceedsMaxPerCall = 8,
    /// `record_usage` was called with a `requests` value below the
    /// configured `MinRequestsPerCall` floor.
    RequestsBelowMinPerCall = 9,
    /// `record_usage` was called by/for an agent not on the allowlist
    /// while strict allowlisting is enabled.
    AgentNotAllowed = 10,
    /// `migrate_v1_to_v2` was called from a non-v1 schema. v2 itself is
    /// already migrated.
    MigrationVersionMismatch = 11,
    /// `record_usage` referenced a service that has been disabled.
    ServiceDisabled = 12,
    /// A metadata-scoped entrypoint referenced a service that has no
    /// `ServiceMetadata` slot set.
    ServiceMetadataNotFound = 13,
    /// `propose_admin_transfer` was called with the current admin as the
    /// proposed new admin — a no-op handover that is rejected to surface
    /// caller mistakes early.
    InvalidAdminProposal = 14,
    /// `record_usage` would push the agent's per-window request count
    /// above the configured `MaxRequestsPerWindow` cap.
    RateLimitExceeded = 15,
    /// `get_usage_batch` was called with more than `MAX_BATCH_READ` pairs.
    BatchTooLarge = 16,
    /// `record_usage` was called by/for an agent on the per-agent
    /// blocklist. Takes precedence over the allowlist.
    AgentBlocked = 17,
    /// `drain_usage_batch` was called with more than `MAX_BATCH_DRAIN` pairs.
    DrainBatchTooLarge = 18,
    /// A service-scoped entrypoint was called with an empty `service_id`
    /// symbol (length 0). An empty id is almost certainly a client
    /// misconfiguration — reject it loudly so the error surfaces early
    /// rather than silently polluting state with a meaningless key.
    InvalidServiceId = 19,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageRecord {
    pub agent: Address,
    pub service_id: Symbol,
    pub requests: u32,
}

// New persistent boolean flags should be read/written via `read_flag` /
// `write_flag` so they inherit the `unwrap_or(false)` default convention.

/// Read a persistent boolean flag, defaulting to `false` when unset.
/// Centralises the `unwrap_or(false)` convention so a new flag can never
/// accidentally default to `true` or skip a check.
fn read_flag(env: &Env, key: &DataKey) -> bool {
    env.storage().persistent().get(key).unwrap_or(false)
}

/// Write a persistent boolean flag.
fn write_flag(env: &Env, key: &DataKey, value: bool) {
    env.storage().persistent().set(key, &value);
}

/// Panics with ContractPaused if the contract is paused. Centralises the
/// emergency-stop check so new mutating entrypoints cannot forget it.
fn ensure_not_paused(env: &Env) {
    if read_flag(env, &DataKey::Paused) {
        panic_with_error!(env, EscrowError::ContractPaused);
    }
}

/// Panics with [`EscrowError::InvalidServiceId`] if `service_id` is the
/// empty symbol (length == 0). Applied at the top of every service-mutating
/// entrypoint — `register_service`, `register_service_with_metadata`,
/// `set_service_price`, `set_service_metadata`, `set_service_disabled`, and
/// `record_usage` — so an unset or blank id is caught before any storage
/// write. Non-empty ids pass through unchanged.
fn ensure_valid_service_id(env: &Env, service_id: &Symbol) {
    if *service_id == Symbol::new(env, "") {
        panic_with_error!(env, EscrowError::InvalidServiceId);
    }
}

/// Persist a service's metadata (`description`, `owner`) under
/// `DataKey::ServiceMetadata(service_id)`. Shared by `set_service_metadata`
/// and `register_service_with_metadata` so the storage shape stays in one
/// place. Performs no auth — callers are responsible for admin gating.
fn write_service_metadata(env: &Env, service_id: &Symbol, description: String, owner: Address) {
    env.storage().persistent().set(
        &DataKey::ServiceMetadata(service_id.clone()),
        &ServiceMetadata { description, owner },
    );
}

/// Read the accumulated usage counter for an `(agent, service_id)` pair,
/// defaulting to `0` when no usage has been recorded. Centralising this
/// read keeps the single-pair `get_usage` and the batched
/// `get_usage_batch` from drifting in their default/key semantics.
fn read_usage(env: &Env, agent: &Address, service_id: &Symbol) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Usage(agent.clone(), service_id.clone()))
        .unwrap_or(0)
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initialize the escrow contract with the operational admin.
    ///
    /// Requires `admin.require_auth()` and panics with
    /// [`EscrowError::AlreadyInitialized`] if an admin has already been stored.
    /// Idempotency is enforced strictly: a second call with the same admin
    /// address still fails. Use a redeploy or a future admin-rotation
    /// entrypoint if the admin needs to change.
    pub fn init(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic_with_error!(&env, EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &CURRENT_SCHEMA);
    }

    /// Returns the admin address stored at `init`, if any.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Record that an agent has consumed usage for a service.
    ///
    /// Accumulates `requests` into the persistent counter keyed by
    /// `(agent, service_id)`. Rejects zero-request calls with
    /// [`EscrowError::RequestsMustBePositive`] so off-chain settlement
    /// loops never see a no-op event in the audit trail. Saturates at
    /// `u32::MAX` rather than overflowing — the settlement loop is
    /// expected to drain the counter long before that becomes plausible.
    ///
    /// Returns a `UsageRecord` carrying the *new total*, not the delta,
    /// so the caller can confirm the post-write state without a second
    /// storage read.
    pub fn record_usage(
        env: Env,
        agent: Address,
        service_id: Symbol,
        requests: u32,
    ) -> UsageRecord {
        // ---- Validation chain (order is part of the public contract) ----
        //
        // Errors MUST fire in this fixed precedence so that client SDKs and
        // off-chain settlement loops can rely on a stable failure ordering.
        // See docs/escrow/validation-order.md for the authoritative reference
        // table with rationale, conditional-read notes, and stability guarantees.
        //
        //   1. Paused            -> #4  ContractPaused          (unconditional)
        //   2. requests == 0     -> #2  RequestsMustBePositive  (unconditional)
        //   3. requests > max    -> #8  RequestsExceedsMaxPerCall (unconditional; default u32::MAX)
        //   4. requests < min    -> #9  RequestsBelowMinPerCall  (unconditional; default 0)
        //   5. registration      -> #7  ServiceNotRegistered     (conditional: RequireServiceRegistration)
        //   6. disabled          -> #12 ServiceDisabled          (unconditional)
        //   7. blocklist         -> #17 AgentBlocked             (unconditional; overrides allowlist)
        //   8. allowlist         -> #10 AgentNotAllowed          (conditional: AllowlistEnabled)
        //   9. rate limit        -> #15 RateLimitExceeded        (conditional: MaxRequestsPerWindow > 0 && WindowSeconds > 0)
        //
        // Conditional-read notes: keys are only read when their controlling
        // flag is active (short-circuited via `&&`):
        //   - ServiceRegistered  only when RequireServiceRegistration is true.
        //   - AgentAllowed       only when AllowlistEnabled is true.
        //   - RateWindow         only when MaxRequestsPerWindow > 0 && WindowSeconds > 0.
        // Unconditionally read every call: Paused, MaxRequestsPerCall,
        // MinRequestsPerCall, ServiceDisabled, AgentBlocked. Each key is read
        // at most once; the max/min caps are cached in locals below.
        // -------------------------------------------------------------------
        ensure_valid_service_id(&env, &service_id);
        if read_flag(&env, &DataKey::Paused) {
            panic_with_error!(&env, EscrowError::ContractPaused);
        }
        if requests == 0 {
            panic_with_error!(&env, EscrowError::RequestsMustBePositive);
        }
        // Cached: read once, compared once. Defaults to u32::MAX (no cap).
        let max_per_call: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MaxRequestsPerCall)
            .unwrap_or(u32::MAX);
        if requests > max_per_call {
            panic_with_error!(&env, EscrowError::RequestsExceedsMaxPerCall);
        }
        // Cached: read once, compared once. Defaults to 0 (no floor).
        let min_per_call: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MinRequestsPerCall)
            .unwrap_or(0);
        if requests < min_per_call {
            panic_with_error!(&env, EscrowError::RequestsBelowMinPerCall);
        }
        // Conditional read: ServiceRegistered is only touched when strict
        // registration is enabled (the `&&` short-circuits otherwise).
        if read_flag(&env, &DataKey::RequireServiceRegistration)
            && !read_flag(&env, &DataKey::ServiceRegistered(service_id.clone()))
        {
            panic_with_error!(&env, EscrowError::ServiceNotRegistered);
        }
        if read_flag(&env, &DataKey::ServiceDisabled(service_id.clone())) {
            panic_with_error!(&env, EscrowError::ServiceDisabled);
        }
        // Per-agent blocklist takes precedence over the allowlist: a blocked
        // agent is rejected even if also allow-listed.
        if read_flag(&env, &DataKey::AgentBlocked(agent.clone())) {
            panic_with_error!(&env, EscrowError::AgentBlocked);
        }
        // Conditional read: AgentAllowed is only touched when the allowlist is
        // enabled (the `&&` short-circuits otherwise).
        if read_flag(&env, &DataKey::AllowlistEnabled)
            && !read_flag(&env, &DataKey::AgentAllowed(agent.clone()))
        {
            panic_with_error!(&env, EscrowError::AgentNotAllowed);
        }

        // Per-agent fixed-window rate limit. Enabled only when both the cap
        // and the window length are non-zero. The window is anchored at the
        // first in-window call's timestamp and rolls forward whole-window
        // (fixed, not sliding) once `now >= window_start + window_seconds`.
        let max_per_window: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MaxRequestsPerWindow)
            .unwrap_or(0);
        let window_seconds: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::WindowSeconds)
            .unwrap_or(0);
        if max_per_window > 0 && window_seconds > 0 {
            let now = env.ledger().timestamp();
            let (window_start, count): (u64, u32) = env
                .storage()
                .persistent()
                .get(&DataKey::RateWindow(agent.clone()))
                .unwrap_or((0, 0));
            // Roll the window forward if the current one has expired. The
            // agent can never reset it early: window_start only advances.
            let (window_start, count) = if now >= window_start.saturating_add(window_seconds) {
                (now, 0u32)
            } else {
                (window_start, count)
            };
            let new_count = count.saturating_add(requests);
            if new_count > max_per_window {
                panic_with_error!(&env, EscrowError::RateLimitExceeded);
            }
            env.storage().persistent().set(
                &DataKey::RateWindow(agent.clone()),
                &(window_start, new_count),
            );
        }

        let key = DataKey::Usage(agent.clone(), service_id.clone());
        let prev: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        // saturate: settlement drains long before u32::MAX; never panic the hot path.
        let total = prev.saturating_add(requests);
        env.storage().persistent().set(&key, &total);

        // Cross-service lifetime counter for the agent. Saturates at u32::MAX.
        let total_key = DataKey::TotalUsageByAgent(agent.clone());
        let prev_total: u32 = env.storage().persistent().get(&total_key).unwrap_or(0);
        // saturate: settlement drains long before u32::MAX; never panic the hot path.
        env.storage()
            .persistent()
            .set(&total_key, &prev_total.saturating_add(requests));

        // Protocol-wide lifetime counter (u64 to delay the saturation horizon).
        let proto_prev: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRequestsAllTime)
            .unwrap_or(0);
        // u64 horizon; saturate not panic.
        env.storage().persistent().set(
            &DataKey::TotalRequestsAllTime,
            &proto_prev.saturating_add(requests as u64),
        );

        env.events().publish(
            (symbol_short!("usage"),),
            (agent.clone(), service_id.clone(), requests, total),
        );

        UsageRecord {
            agent,
            service_id,
            requests: total,
        }
    }

    /// Read the ledger timestamp at which `settle` last drained an
    /// `(agent, service_id)` pair. Returns `None` for pairs that have
    /// never been settled (vs. `Some(0)`, which would be a genesis-block
    /// settlement and should not be confused with absent).
    pub fn get_last_settlement(env: Env, agent: Address, service_id: Symbol) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::LastSettlement(agent, service_id))
    }

    /// Read the protocol-wide lifetime request counter (u64).
    pub fn get_total_requests_all_time(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalRequestsAllTime)
            .unwrap_or(0)
    }

    /// Read the cross-service lifetime request count for an agent.
    /// Not affected by `settle` (which only drains per-service counters).
    pub fn get_total_usage_by_agent(env: Env, agent: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalUsageByAgent(agent))
            .unwrap_or(0)
    }

    /// Returns the accumulated request count for an `(agent, service_id)`
    /// pair, or `0` if no usage has been recorded yet.
    pub fn get_usage(env: Env, agent: Address, service_id: Symbol) -> u32 {
        read_usage(&env, &agent, &service_id)
    }

    /// Batched usage read: returns the accumulated request count for each
    /// input `(agent, service_id)` pair, in the same order as `pairs`.
    ///
    /// Pure read — no `require_auth`, no pause gate — so off-chain
    /// dashboards and settlement loops can fetch many counters in one call.
    /// Each entry is resolved with the same `read_usage` helper as
    /// [`Escrow::get_usage`], so unknown pairs return `0` and duplicate
    /// pairs simply yield the same value at each position.
    ///
    /// Panics with [`EscrowError::BatchTooLarge`] when
    /// `pairs.len() > MAX_BATCH_READ`. Rejecting oversized requests keeps
    /// the read loop bounded and the host's storage-read budget
    /// predictable; callers should page larger queries.
    pub fn get_usage_batch(env: Env, pairs: Vec<(Address, Symbol)>) -> Vec<u32> {
        if pairs.len() > MAX_BATCH_READ {
            panic_with_error!(&env, EscrowError::BatchTooLarge);
        }
        let mut results: Vec<u32> = Vec::new(&env);
        for (agent, service_id) in pairs.iter() {
            results.push_back(read_usage(&env, &agent, &service_id));
        }
        results
    }

    /// Batched service-status read: returns a [`ServiceStatus`] for each input
    /// `service_id`, in the same order as `service_ids`.
    ///
    /// Pure read — no `require_auth`, no pause gate — consistent with the
    /// other single-service getters. Each entry reuses the same per-service
    /// flag/price reads as [`Escrow::is_service_registered`],
    /// [`Escrow::is_service_disabled`], and [`Escrow::get_service_price`], so
    /// the batched and individual paths cannot drift. Unknown services (never
    /// registered, disabled, or priced) produce an entry with
    /// `registered = false`, `disabled = false`, and `price_stroops = 0`.
    ///
    /// Panics with [`EscrowError::BatchTooLarge`] when
    /// `service_ids.len() > MAX_SERVICE_STATUS_BATCH`. Rejecting oversized
    /// requests keeps the read loop bounded and the host's storage-read budget
    /// predictable; callers should page larger queries.
    pub fn get_services_status_batch(
        env: Env,
        service_ids: Vec<Symbol>,
    ) -> Vec<ServiceStatus> {
        if service_ids.len() > MAX_SERVICE_STATUS_BATCH {
            panic_with_error!(&env, EscrowError::BatchTooLarge);
        }
        let mut results: Vec<ServiceStatus> = Vec::new(&env);
        for service_id in service_ids.iter() {
            let registered = read_flag(&env, &DataKey::ServiceRegistered(service_id.clone()));
            let disabled = read_flag(&env, &DataKey::ServiceDisabled(service_id.clone()));
            let price_stroops: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::ServicePrice(service_id.clone()))
                .unwrap_or(0);
            results.push_back(ServiceStatus {
                service_id,
                registered,
                disabled,
                price_stroops,
            });
        }
        results
    }

    /// Set the per-request price (in stroops) for a service.
    ///
    /// Admin-gated. Persists in `DataKey::ServicePrice(service_id)`.
    /// A negative price is rejected at call time so downstream billing
    /// math can assume a non-negative multiplicand; a zero price is
    /// allowed and means "free service" (still records usage, settles to
    /// zero).
    ///
    /// Registration coupling: when `RequireServiceRegistration` (the same
    /// strict-mode flag enforced by `record_usage`) is enabled, a price
    /// can only attach to a registered `service_id` — otherwise the call
    /// is rejected with [`EscrowError::ServiceNotRegistered`]. With the
    /// flag off (the default), pricing is unrestricted, preserving the
    /// prior backward-compatible behaviour.
    ///
    /// A disabled service is always rejected with
    /// [`EscrowError::ServiceDisabled`], mirroring `record_usage`'s gate,
    /// so prices cannot drift onto services that are out of commission.
    ///
    /// Emits `price_set(service_id, price_stroops)` only after every
    /// validation passes.
    pub fn set_service_price(env: Env, service_id: Symbol, price_stroops: i128) {
        ensure_valid_service_id(&env, &service_id);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        admin.require_auth();
        ensure_not_paused(&env);
        if price_stroops < 0 {
            panic_with_error!(&env, EscrowError::RequestsMustBePositive);
        }
        if read_flag(&env, &DataKey::RequireServiceRegistration)
            && !read_flag(&env, &DataKey::ServiceRegistered(service_id.clone()))
        {
            panic_with_error!(&env, EscrowError::ServiceNotRegistered);
        }
        if read_flag(&env, &DataKey::ServiceDisabled(service_id.clone())) {
            panic_with_error!(&env, EscrowError::ServiceDisabled);
        }
        env.storage()
            .persistent()
            .set(&DataKey::ServicePrice(service_id.clone()), &price_stroops);
        env.events()
            .publish((symbol_short!("price_set"),), (service_id, price_stroops));
    }

    /// Remove the configured per-request price for a service, freeing the
    /// `DataKey::ServicePrice(service_id)` storage slot.
    ///
    /// Admin-gated and honours the pause gate (panics with
    /// [`EscrowError::ContractPaused`] when paused, consistent with other
    /// admin mutations). Idempotent — removing the price of a service that
    /// was never priced is a no-op.
    ///
    /// After removal, `get_service_price` and `compute_billing` read back
    /// `0`, exactly as for a service that was never priced. Note the
    /// zero-vs-removed distinction: removal frees the underlying storage
    /// slot and emits a `price_rm` event, whereas `set_service_price(_, 0)`
    /// leaves a stored slot holding `0`. Both read back as `0`, but only
    /// removal reclaims the slot. Emits `price_rm(service_id)`.
    pub fn remove_service_price(env: Env, service_id: Symbol) {
        ensure_not_paused(&env);
        require_admin(&env);
        env.storage()
            .persistent()
            .remove(&DataKey::ServicePrice(service_id.clone()));
        env.events()
            .publish((symbol_short!("price_rm"),), service_id);
    }

    /// Get the per-request price (in stroops) for a service, or 0 if
    /// no price has been configured (the service is free / unset).
    pub fn get_service_price(env: Env, service_id: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::ServicePrice(service_id))
            .unwrap_or(0)
    }

    /// Compute the outstanding bill for an `(agent, service_id)` pair:
    /// `accumulated_requests * price_per_request`, in stroops.
    ///
    /// Returns 0 when either side is zero. Saturates at `i128::MAX` on
    /// overflow — this is read-only, so a saturated value just signals
    /// to the off-chain settlement loop that something has gone wrong
    /// rather than panicking the host.
    pub fn compute_billing(env: Env, agent: Address, service_id: Symbol) -> i128 {
        let requests: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::Usage(agent, service_id.clone()))
            .unwrap_or(0);
        let price: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::ServicePrice(service_id))
            .unwrap_or(0);
        // saturate: read/settle path returns a sentinel-large value rather than
        // panicking; off-chain loop treats saturation as an error signal.
        (requests as i128).saturating_mul(price)
    }

    /// Settle the accumulated usage for an `(agent, service_id)` pair.
    ///
    /// Authorised by `caller`, which must be **either** the global admin
    /// **or** the `ServiceMetadata(service_id).owner` for that specific
    /// service. This lets a registered service owner trigger settlement
    /// for their own service without holding the central admin key, while
    /// still allowing the admin to settle anything.
    ///
    /// Authorization matrix:
    /// - `caller == admin` → always allowed.
    /// - `caller == owner` of this service → allowed.
    /// - service has no metadata/owner and `caller != admin` →
    ///   [`EscrowError::ServiceMetadataNotFound`].
    /// - `caller` is some other address → [`EscrowError::NotPendingAdmin`]
    ///   (reused as the unauthorized-caller error, matching
    ///   `transfer_service_ownership`).
    ///
    /// Computes the outstanding bill (same math as `compute_billing`),
    /// resets the usage counter to zero, stamps `LastSettlement`, and
    /// returns the billed amount in stroops. Honours the pause gate and
    /// emits the `settled` event identically to before.
    pub fn settle(env: Env, caller: Address, agent: Address, service_id: Symbol) -> i128 {
        ensure_not_paused(&env);
        caller.require_auth();
        let admin = get_admin_address(&env);
        if caller != admin {
            // Non-admin caller must be the registered owner of this service.
            let meta: ServiceMetadata = env
                .storage()
                .persistent()
                .get(&DataKey::ServiceMetadata(service_id.clone()))
                .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ServiceMetadataNotFound));
            if caller != meta.owner {
                panic_with_error!(&env, EscrowError::NotPendingAdmin);
            }
        }
        let usage_key = DataKey::Usage(agent.clone(), service_id.clone());
        let requests: u32 = env.storage().persistent().get(&usage_key).unwrap_or(0);
        let price: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::ServicePrice(service_id.clone()))
            .unwrap_or(0);
        // saturate: read/settle path returns a sentinel-large value rather than
        // panicking; off-chain loop treats saturation as an error signal.
        let billed = (requests as i128).saturating_mul(price);
        env.storage().persistent().set(&usage_key, &0u32);
        env.storage().persistent().set(
            &DataKey::LastSettlement(agent.clone(), service_id.clone()),
            &env.ledger().timestamp(),
        );
        env.events().publish(
            (symbol_short!("settled"),),
            (agent, service_id, requests, billed),
        );
        billed
    }

    /// Read the configured per-call floor, or `0` (no floor) when absent.
    pub fn get_min_requests_per_call(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MinRequestsPerCall)
            .unwrap_or(0)
    }

    /// Admin enables or disables the agent allowlist gate. While
    /// disabled, `record_usage` does not consult the per-agent entries.
    pub fn set_allowlist_enabled(env: Env, enabled: bool) {
        require_admin(&env);
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::AllowlistEnabled, enabled);
    }

    /// Read the master allowlist toggle.
    pub fn is_allowlist_enabled(env: Env) -> bool {
        read_flag(&env, &DataKey::AllowlistEnabled)
    }

    /// Read whether an agent is explicitly allowed (false for never-set).
    pub fn is_agent_allowed(env: Env, agent: Address) -> bool {
        read_flag(&env, &DataKey::AgentAllowed(agent))
    }

    /// Admin sets the allowlist status for a specific agent.
    pub fn set_agent_allowed(env: Env, agent: Address, allowed: bool) {
        require_admin(&env);
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::AgentAllowed(agent), allowed);
    }

    /// Read whether an agent is on the blocklist (false for never-set).
    pub fn is_agent_blocked(env: Env, agent: Address) -> bool {
        read_flag(&env, &DataKey::AgentBlocked(agent))
    }

    /// Admin sets the blocklist status for a specific agent. A blocked
    /// agent is rejected by `record_usage` with `AgentBlocked`,
    /// independent of the allowlist and taking precedence over it: an
    /// agent that is both allow-listed and blocked is still rejected.
    pub fn set_agent_blocked(env: Env, agent: Address, blocked: bool) {
        require_admin(&env);
        write_flag(&env, &DataKey::AgentBlocked(agent), blocked);
    }

    /// Admin sets the per-call lower bound on `requests` for batched
    /// writes. Pass `0` to disable the floor.
    pub fn set_min_requests_per_call(env: Env, min_requests: u32) {
        require_admin(&env);
        ensure_not_paused(&env);
        env.storage()
            .persistent()
            .set(&DataKey::MinRequestsPerCall, &min_requests);
    }

    /// Read the configured per-call cap, or `u32::MAX` (no limit) if
    /// none has been set.
    pub fn get_max_requests_per_call(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxRequestsPerCall)
            .unwrap_or(u32::MAX)
    }

    /// Read the configured per-window request cap, or `0` (limiter
    /// disabled) when unset.
    pub fn get_max_requests_per_window(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxRequestsPerWindow)
            .unwrap_or(0)
    }

    /// Admin sets the per-agent, per-window request cap. The limiter is
    /// active only when both this cap and the window length
    /// ([`Self::set_rate_window_seconds`]) are non-zero. Pass `0` to
    /// disable.
    pub fn set_max_requests_per_window(env: Env, max_requests: u32) {
        require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::MaxRequestsPerWindow, &max_requests);
    }

    /// Read the configured rate-limit window length in seconds, or `0`
    /// (limiter disabled) when unset.
    pub fn get_rate_window_seconds(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::WindowSeconds)
            .unwrap_or(0)
    }

    /// Admin sets the fixed rate-limit window length in seconds. The
    /// limiter is active only when both this and the per-window cap are
    /// non-zero. Pass `0` to disable.
    pub fn set_rate_window_seconds(env: Env, window_seconds: u64) {
        require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::WindowSeconds, &window_seconds);
    }

    /// Admin sets the per-call upper bound on `requests` accepted by
    /// `record_usage`. Pass `u32::MAX` to effectively disable the cap.
    pub fn set_max_requests_per_call(env: Env, max_requests: u32) {
        require_admin(&env);
        ensure_not_paused(&env);
        env.storage()
            .persistent()
            .set(&DataKey::MaxRequestsPerCall, &max_requests);
    }

    /// Admin toggles strict-registration mode. When enabled,
    /// `record_usage` rejects unknown services with
    /// EscrowError::ServiceNotRegistered.
    pub fn set_require_service_registration(env: Env, required: bool) {
        require_admin(&env);
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::RequireServiceRegistration, required);
    }

    /// Read the strict-registration flag.
    pub fn is_service_registration_required(env: Env) -> bool {
        read_flag(&env, &DataKey::RequireServiceRegistration)
    }

    /// Read whether a service has been registered.
    pub fn is_service_registered(env: Env, service_id: Symbol) -> bool {
        read_flag(&env, &DataKey::ServiceRegistered(service_id))
    }

    /// Unregister a service. Admin-gated; idempotent (removing an absent
    /// entry is a no-op). Existing usage records and prices for the
    /// service are NOT touched — call reset_usage or remove the price
    /// separately if a clean wipe is required.
    pub fn unregister_service(env: Env, service_id: Symbol) {
        require_admin(&env);
        ensure_not_paused(&env);
        env.storage()
            .persistent()
            .remove(&DataKey::ServiceRegistered(service_id));
    }

    /// Register a service so `record_usage` accepts it under strict
    /// registration. Admin-gated and idempotent.
    pub fn register_service(env: Env, service_id: Symbol) {
        ensure_valid_service_id(&env, &service_id);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        admin.require_auth();
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::ServiceRegistered(service_id), true);
    }

    /// Cancel a pending admin transfer. Current admin only. No-op when
    /// nothing is pending.
    pub fn cancel_admin_transfer(env: Env) {
        require_admin(&env);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
    }

    /// Read the pending admin, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::PendingAdmin)
    }

    /// Step 2 of admin handover. The pending admin (set by step 1)
    /// claims the role; this proves they control the key. Panics with
    /// NoPendingAdminTransfer if none is pending, or NotPendingAdmin
    /// if the caller does not match the pending entry.
    pub fn accept_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoPendingAdminTransfer));
        if pending != caller {
            panic_with_error!(&env, EscrowError::NotPendingAdmin);
        }
        env.storage().persistent().set(&DataKey::Admin, &caller);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
    }

    /// Step 1 of admin handover. Current admin proposes a new admin
    /// address; the new admin must then call `accept_admin_transfer`
    /// from their own key to finish the rotation. Re-proposing
    /// overwrites the prior pending entry.
    pub fn propose_admin_transfer(env: Env, new_admin: Address) {
        let admin = require_admin(&env);
        if new_admin == admin {
            panic_with_error!(&env, EscrowError::InvalidAdminProposal);
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
    }

    /// Returns `true` iff the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        read_flag(&env, &DataKey::Paused)
    }

    /// Resume operations after a previous `pause()`. Admin-gated and
    /// idempotent (unpausing an already-unpaused contract is a no-op).
    pub fn unpause(env: Env) {
        require_admin(&env);
        write_flag(&env, &DataKey::Paused, false);
        env.events().publish((symbol_short!("paused"),), false);
    }

    /// Pause the contract — every state-changing entrypoint will then
    /// panic with [`EscrowError::ContractPaused`]. Admin-gated and
    /// idempotent (pausing an already-paused contract is a no-op write).
    pub fn pause(env: Env) {
        require_admin(&env);
        write_flag(&env, &DataKey::Paused, true);
        env.events().publish((symbol_short!("paused"),), true);
    }

    /// Migrate the persisted schema from v1 to v2. Admin-gated and
    /// idempotent in shape — but panics with `MigrationVersionMismatch`
    /// if the current schema is already at v2 (or higher), to surface
    /// accidental double-runs. All v2 reads default sensibly when their
    /// new slots are absent, so the migration body itself only stamps
    /// the new SchemaVersion; no data fan-out is required.
    pub fn migrate_v1_to_v2(env: Env) {
        require_admin(&env);
        let current: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(1);
        if current != 1 {
            panic_with_error!(&env, EscrowError::MigrationVersionMismatch);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &2u32);
    }

    /// Read the metadata for a service, or `None` if none has been set.
    pub fn get_service_metadata(env: Env, service_id: Symbol) -> Option<ServiceMetadata> {
        env.storage()
            .persistent()
            .get(&DataKey::ServiceMetadata(service_id))
    }

    /// Returns `true` iff the service has been disabled.
    pub fn is_service_disabled(env: Env, service_id: Symbol) -> bool {
        read_flag(&env, &DataKey::ServiceDisabled(service_id))
    }

    /// Admin sets the disabled flag for a service. Disabling a service
    /// causes `record_usage` to panic with `ServiceDisabled` for that
    /// id; registration and metadata are preserved.
    pub fn set_service_disabled(env: Env, service_id: Symbol, disabled: bool) {
        ensure_valid_service_id(&env, &service_id);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        admin.require_auth();
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::ServiceDisabled(service_id), disabled);
    }

    /// Admin sets human-readable metadata for a service. Persisted
    /// under `DataKey::ServiceMetadata(service_id)`. Description is
    /// capped at 256 UTF-8 bytes to bound storage cost.
    pub fn set_service_metadata(env: Env, service_id: Symbol, description: String, owner: Address) {
        ensure_valid_service_id(&env, &service_id);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        admin.require_auth();
        ensure_not_paused(&env);
        write_service_metadata(&env, &service_id, description, owner);
    }

    /// Register a service and set its metadata in a single admin-gated,
    /// atomic call. Equivalent to calling `register_service` followed by
    /// `set_service_metadata`, but with one auth check and one event so
    /// indexers see registration and metadata land together.
    ///
    /// Sets `ServiceRegistered(service_id) = true` and persists
    /// `ServiceMetadata(service_id)` (`description` + `owner`). Idempotent:
    /// re-registering an existing id overwrites its metadata. An empty
    /// `description` is accepted. Emits `svc_reg(service_id, owner)`.
    pub fn register_service_with_metadata(
        env: Env,
        service_id: Symbol,
        description: String,
        owner: Address,
    ) {
        ensure_valid_service_id(&env, &service_id);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        admin.require_auth();
        ensure_not_paused(&env);
        write_flag(&env, &DataKey::ServiceRegistered(service_id.clone()), true);
        write_service_metadata(&env, &service_id, description, owner.clone());
        env.events()
            .publish((symbol_short!("svc_reg"),), (service_id, owner));
    }

    /// Transfer ownership of a service's metadata to `new_owner`,
    /// preserving the existing `description`. Authorised by `caller`,
    /// which must be the current owner OR the admin. Panics with
    /// `ServiceMetadataNotFound` if no metadata has been set. Emits
    /// `owner_chg(service_id, old_owner, new_owner)` for indexers.
    /// Honours the pause gate.
    pub fn transfer_service_ownership(
        env: Env,
        caller: Address,
        service_id: Symbol,
        new_owner: Address,
    ) {
        ensure_not_paused(&env);
        caller.require_auth();
        let admin = get_admin_address(&env);
        let mut meta: ServiceMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::ServiceMetadata(service_id.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ServiceMetadataNotFound));
        if caller != meta.owner && caller != admin {
            panic_with_error!(&env, EscrowError::NotPendingAdmin); // reuse: unauthorized caller
        }
        let old_owner = meta.owner.clone();
        meta.owner = new_owner.clone();
        env.storage()
            .persistent()
            .set(&DataKey::ServiceMetadata(service_id.clone()), &meta);
        env.events().publish(
            (symbol_short!("owner_chg"),),
            (service_id, old_owner, new_owner),
        );
    }

    /// Admin-gated. Remove a service's metadata (description + owner).
    /// Idempotent — clearing an absent entry is a no-op. After clearing,
    /// `get_service_metadata` reads back `None`. Registration and usage
    /// history live in independent slots and are untouched. Emits
    /// `meta_clr(service_id)` (topic shortened to satisfy the 9-char
    /// `symbol_short!` limit).
    pub fn clear_service_metadata(env: Env, service_id: Symbol) {
        require_admin(&env);
        ensure_not_paused(&env);
        env.storage()
            .persistent()
            .remove(&DataKey::ServiceMetadata(service_id.clone()));
        env.events()
            .publish((symbol_short!("meta_clr"),), service_id);
    }

    /// Read the on-chain schema version, or `1` (the implicit
    /// pre-migration default) if absent.
    pub fn get_schema_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(1)
    }

    /// Get the version of the contract for compatibility checks.
    ///
    /// v2 adds pause/unpause, two-step admin handover, service registry,
    /// per-call min/max bounds, an agent allowlist, lifetime usage
    /// counters, settlement-time tracking, and a stored schema version.
    pub fn version(env: Env) -> u32 {
        let _ = env;
        2
    }
}

#[cfg(test)]
mod test;
