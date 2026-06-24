#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, IntoVal, Symbol,
};

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
    assert_eq!(v, 2);
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
    // First write: total equals the recorded delta.
    assert_eq!(record.requests, requests);
}

#[test]
fn test_record_usage_accumulates_across_calls() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");

    let first = client.record_usage(&agent, &service_id, &40u32);
    assert_eq!(first.requests, 40);
    let second = client.record_usage(&agent, &service_id, &60u32);
    assert_eq!(second.requests, 100);
    let third = client.record_usage(&agent, &service_id, &1u32);
    assert_eq!(third.requests, 101);

    assert_eq!(client.get_usage(&agent, &service_id), 101);
}

#[test]
fn test_record_usage_is_keyed_per_service() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let weather = Symbol::new(&env, "weather_api");
    let inference = Symbol::new(&env, "infer_api");

    client.record_usage(&agent, &weather, &10u32);
    client.record_usage(&agent, &inference, &7u32);

    assert_eq!(client.get_usage(&agent, &weather), 10);
    assert_eq!(client.get_usage(&agent, &inference), 7);
}

#[test]
fn test_get_usage_returns_zero_for_unknown_pair() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let unseen_agent = Address::generate(&env);
    let svc = Symbol::new(&env, "anything");
    assert_eq!(client.get_usage(&unseen_agent, &svc), 0);
}

#[test]
fn test_set_service_price_admin_can_write() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.set_service_price(&Symbol::new(&env, "infer"), &500i128);
}

#[test]
fn test_get_service_price_round_trip() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    assert_eq!(client.get_service_price(&svc), 500i128);
}

#[test]
fn test_get_service_price_defaults_to_zero() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    assert_eq!(client.get_service_price(&Symbol::new(&env, "never_set")), 0i128);
}

#[test]
fn test_compute_billing_basic() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);
    assert_eq!(client.compute_billing(&agent, &svc), 420i128);
}

#[test]
fn test_settle_drains_usage_and_returns_billed() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);
    let billed = client.settle(&agent, &svc);
    assert_eq!(billed, 420i128);
    assert_eq!(client.get_usage(&agent, &svc), 0);
}

#[test]
fn test_pause_admin_can_pause() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
}

#[test]
fn test_unpause_admin_can_unpause() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    client.unpause();
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_settle_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.settle(&agent, &Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_usage_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &1u32);
}

#[test]
fn test_propose_admin_transfer_persists_pending() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
}

#[test]
fn test_accept_admin_transfer_rotates_admin() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    client.accept_admin_transfer(&next);
    assert_eq!(client.get_admin(), Some(next));
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_accept_admin_transfer_panics_with_no_pending() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let caller = Address::generate(&env);
    client.accept_admin_transfer(&caller);
}

#[test]
fn test_is_paused_round_trip() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_settle_returns_zero_for_unused_pair() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    assert_eq!(client.settle(&agent, &svc), 0i128);
}

#[test]
fn test_compute_billing_zero_when_unpriced_or_unused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    // no price, no usage
    assert_eq!(client.compute_billing(&agent, &svc), 0i128);
    client.record_usage(&agent, &svc, &10u32);
    // usage > 0 but price still 0
    assert_eq!(client.compute_billing(&agent, &svc), 0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_record_usage_rejects_zero_requests() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &0u32);
}

#[test]
fn test_bool_flag_accessor_round_trip() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    // Defaults to false when unset.
    assert!(!client.is_allowlist_enabled());
    // Round-trips true then false through the centralised accessors.
    client.set_allowlist_enabled(&true);
    assert!(client.is_allowlist_enabled());
    client.set_allowlist_enabled(&false);
    assert!(!client.is_allowlist_enabled());
}

#[test]
fn test_transfer_service_ownership_by_owner_preserves_description() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let desc = String::from_str(&env, "inference service");
    client.set_service_metadata(&svc, &desc, &owner);

    client.transfer_service_ownership(&owner, &svc, &new_owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.owner, new_owner);
    assert_eq!(meta.description, desc);
}

#[test]
fn test_transfer_service_ownership_by_admin() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let desc = String::from_str(&env, "inference service");
    client.set_service_metadata(&svc, &desc, &owner);

    client.transfer_service_ownership(&admin, &svc, &new_owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.owner, new_owner);
    assert_eq!(meta.description, desc);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_transfer_service_ownership_missing_metadata_panics() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    let caller = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.transfer_service_ownership(&caller, &svc, &new_owner);
}

#[test]
fn test_clear_service_metadata_removes_entry() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let desc = String::from_str(&env, "inference service");
    client.set_service_metadata(&svc, &desc, &owner);
    assert!(client.get_service_metadata(&svc).is_some());

    client.clear_service_metadata(&svc);
    assert!(client.get_service_metadata(&svc).is_none());
}

#[test]
fn test_clear_service_metadata_is_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    // Clearing a never-set entry is a no-op (no panic).
    client.clear_service_metadata(&svc);
    assert!(client.get_service_metadata(&svc).is_none());
}

#[test]
fn test_clear_service_metadata_leaves_registration_untouched() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let desc = String::from_str(&env, "inference service");
    client.register_service(&svc);
    client.set_service_metadata(&svc, &desc, &owner);

    client.clear_service_metadata(&svc);

    assert!(client.get_service_metadata(&svc).is_none());
    assert!(client.is_service_registered(&svc));
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_propose_admin_transfer_rejects_self_target() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.propose_admin_transfer(&admin);
}

#[test]
fn test_propose_admin_transfer_accepts_distinct_address() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    assert_eq!(client.get_pending_admin(), Some(next));
}

#[test]
fn test_accept_admin_transfer_clears_pending() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    client.accept_admin_transfer(&next);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn test_settle_drains_to_zero_and_stamps_last_settlement() {
    let env = Env::default();
    let ts: u64 = 12345;
    env.ledger().with_mut(|li| li.timestamp = ts);

    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);

    // No settlement has happened yet for this pair.
    assert_eq!(client.get_last_settlement(&agent, &svc), None);

    let billed = client.settle(&agent, &svc);

    assert_eq!(billed, 420i128);
    // Usage drains to exactly zero.
    assert_eq!(client.get_usage(&agent, &svc), 0);
    // LastSettlement is stamped with the current ledger timestamp.
    assert_eq!(client.get_last_settlement(&agent, &svc), Some(ts));
}

#[test]
fn test_settle_billed_matches_compute_billing_for_presettle_state() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &7i128);
    client.record_usage(&agent, &svc, &13u32);

    // Capture the bill the contract would report for the pre-settle state.
    let expected = client.compute_billing(&agent, &svc);
    assert_eq!(expected, 91i128);

    let billed = client.settle(&agent, &svc);
    assert_eq!(billed, expected);
    // And compute_billing now reads zero since usage drained.
    assert_eq!(client.compute_billing(&agent, &svc), 0i128);
}

#[test]
fn test_settle_emits_settled_event_with_payload() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);

    let billed = client.settle(&agent, &svc);

    let events = env.events().all();
    assert!(!events.is_empty());
    // The settled event is the most recent publish: (contract, topics, data).
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("settled"),).into_val(&env);
    // Topics is a Vec<Val> with a reliable structural PartialEq.
    assert_eq!(topics, expected_topics);
    // Decode the data payload back into typed values and assert the tuple.
    let decoded: (Address, Symbol, u32, i128) = data.into_val(&env);
    assert_eq!(decoded, (agent.clone(), svc.clone(), 42u32, billed));
}

#[test]
fn test_record_usage_emits_usage_event_with_payload() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "weather_api");

    let record = client.record_usage(&agent, &svc, &25u32);

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("usage"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    // Payload is (agent, service_id, requests_delta, new_total).
    let decoded: (Address, Symbol, u32, u32) = data.into_val(&env);
    assert_eq!(
        decoded,
        (agent.clone(), svc.clone(), 25u32, record.requests)
    );
}

#[test]
fn test_settle_zero_usage_returns_zero_stamps_and_emits_event() {
    let env = Env::default();
    let ts: u64 = 99_999;
    env.ledger().with_mut(|li| li.timestamp = ts);

    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);

    // Settle a pair that never recorded any usage.
    let billed = client.settle(&agent, &svc);
    assert_eq!(billed, 0i128);

    // Capture events immediately after `settle`: `events().all()` only
    // surfaces events from the most recent contract invocation, so any
    // intervening read (e.g. get_last_settlement) would clear them.
    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("settled"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    let decoded: (Address, Symbol, u32, i128) = data.into_val(&env);
    assert_eq!(decoded, (agent.clone(), svc.clone(), 0u32, 0i128));

    // Still stamps LastSettlement so SLA monitors see the drain ran.
    assert_eq!(client.get_last_settlement(&agent, &svc), Some(ts));
}

#[test]
fn test_init_stamps_schema_version() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_migrate_v1_to_v2_rejected_on_fresh_v2_init() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.migrate_v1_to_v2();
}

#[test]
fn test_set_service_metadata_round_trips_description_and_owner() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");

    client.set_service_metadata(&svc, &description, &owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, description);
    assert_eq!(meta.owner, owner);
}

#[test]
fn test_get_service_metadata_returns_none_when_never_set() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    assert_eq!(client.get_service_metadata(&svc), None);
}

#[test]
fn test_register_service_does_not_set_disabled_flag() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");

    client.register_service(&svc);

    assert!(client.is_service_registered(&svc));
    // Registering must not implicitly disable the service.
    assert!(!client.is_service_disabled(&svc));
}

#[test]
fn test_disable_preserves_registration_and_metadata() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");

    client.register_service(&svc);
    client.set_service_metadata(&svc, &description, &owner);

    client.set_service_disabled(&svc, &true);

    // Disabling a service is orthogonal to registration and metadata.
    assert!(client.is_service_disabled(&svc));
    assert!(client.is_service_registered(&svc));
    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, description);
    assert_eq!(meta.owner, owner);
}

#[test]
fn test_unregister_service_does_not_clear_metadata_or_disabled_flag() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");

    client.register_service(&svc);
    client.set_service_metadata(&svc, &description, &owner);
    client.set_service_disabled(&svc, &true);

    client.unregister_service(&svc);

    // unregister_service only removes the ServiceRegistered slot.
    assert!(!client.is_service_registered(&svc));
    // Metadata and the disabled flag survive an unregister.
    assert!(client.is_service_disabled(&svc));
    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, description);
    assert_eq!(meta.owner, owner);
}

#[test]
fn test_service_slot_toggle_matrix_is_independent() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");

    // Baseline: every slot reads its default for a fresh service id.
    assert!(!client.is_service_registered(&svc));
    assert!(!client.is_service_disabled(&svc));
    assert_eq!(client.get_service_metadata(&svc), None);

    // Toggle registered only.
    client.register_service(&svc);
    assert!(client.is_service_registered(&svc));
    assert!(!client.is_service_disabled(&svc));

    // Toggle disabled only; registered stays set.
    client.set_service_disabled(&svc, &true);
    assert!(client.is_service_registered(&svc));
    assert!(client.is_service_disabled(&svc));

    // Re-enable; registered stays set.
    client.set_service_disabled(&svc, &false);
    assert!(client.is_service_registered(&svc));
    assert!(!client.is_service_disabled(&svc));
}

#[test]
fn test_pause_emits_paused_event_true() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    client.pause();

    // Read events immediately after pause(): events().all() only surfaces
    // events from the most recent contract invocation.
    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("paused"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    let flag: bool = data.into_val(&env);
    assert!(flag);
    assert!(client.is_paused());
}

#[test]
fn test_unpause_emits_paused_event_false() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();

    client.unpause();

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("paused"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    let flag: bool = data.into_val(&env);
    assert!(!flag);
    assert!(!client.is_paused());
}

#[test]
fn test_double_pause_is_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    client.pause();
    assert!(client.is_paused());
    // Pausing an already-paused contract keeps it paused.
    client.pause();
    assert!(client.is_paused());
}

#[test]
fn test_double_unpause_is_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    // Unpausing a never-paused contract is a no-op and stays unpaused.
    client.unpause();
    assert!(!client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_pause_unpause_ends_unpaused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

    client.pause();
    client.pause();
    client.unpause();

    assert!(!client.is_paused());
}
