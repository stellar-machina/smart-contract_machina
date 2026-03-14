#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

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
