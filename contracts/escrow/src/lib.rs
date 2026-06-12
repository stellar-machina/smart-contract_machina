#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Symbol,
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
    /// Initialize the escrow contract.
    pub fn init(env: Env) {
        // Placeholder for one-time init (e.g. admin) if needed.
        let _ = env;
    }

    /// Record that an agent has consumed usage for a service.
    /// In a full implementation this would update balances and trigger settlement.
    pub fn record_usage(
        _env: Env,
        agent: Address,
        service_id: Symbol,
        requests: u32,
    ) -> UsageRecord {
        UsageRecord {
            agent: agent.clone(),
            service_id,
            requests,
        }
    }

    /// Get the version of the contract for compatibility checks.
    pub fn version(env: Env) -> u32 {
        let _ = env;
        1
    }
}

#[cfg(test)]
mod test;
