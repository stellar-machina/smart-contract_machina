#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
};

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
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageRecord {
    pub agent: Address,
    pub service_id: Symbol,
    pub requests: u32,
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
        if env
            .storage()
            .persistent()
            .has(&DataKey::Admin)
        {
            panic_with_error!(&env, EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
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
        if requests == 0 {
            panic_with_error!(&env, EscrowError::RequestsMustBePositive);
        }
        let key = DataKey::Usage(agent.clone(), service_id.clone());
        let prev: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        let total = prev.saturating_add(requests);
        env.storage().persistent().set(&key, &total);
        UsageRecord {
            agent,
            service_id,
            requests: total,
        }
    }

    /// Returns the accumulated request count for an `(agent, service_id)`
    /// pair, or `0` if no usage has been recorded yet.
    pub fn get_usage(env: Env, agent: Address, service_id: Symbol) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::Usage(agent, service_id))
            .unwrap_or(0)
    }

    /// Get the version of the contract for compatibility checks.
    pub fn version(env: Env) -> u32 {
        let _ = env;
        1
    }
}

#[cfg(test)]
mod test;
