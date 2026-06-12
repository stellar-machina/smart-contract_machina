#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Symbol};

fn setup_initialized(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);
    (client, admin)
}

#[test]
fn test_version() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let v = client.version();
    assert_eq!(v, 1);
}

#[test]
fn test_init_persists_admin() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_init_rejects_double_init() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let other_admin = Address::generate(&env);
    client.init(&other_admin);
}

#[test]
fn test_record_usage() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    let requests: u32 = 100;

    let record = client.record_usage(&agent, &service_id, &requests);
    assert_eq!(record.agent, agent);
    assert_eq!(record.service_id, service_id);
    assert_eq!(record.requests, requests);
}
