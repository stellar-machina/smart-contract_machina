#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
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
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let other_admin = Address::generate(&env);
    client.init(&other_admin);
}

#[test]
fn test_record_usage() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

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
    let (client, admin) = setup_initialized(&env);

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
    let (client, admin) = setup_initialized(&env);

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
    let (client, admin) = setup_initialized(&env);
    let unseen_agent = Address::generate(&env);
    let svc = Symbol::new(&env, "anything");
    assert_eq!(client.get_usage(&unseen_agent, &svc), 0);
}

#[test]
fn test_set_service_price_admin_can_write() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_service_price(&Symbol::new(&env, "infer"), &500i128);
}

#[test]
fn test_get_service_price_round_trip() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    assert_eq!(client.get_service_price(&svc), 500i128);
}

#[test]
fn test_get_service_price_defaults_to_zero() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    assert_eq!(
        client.get_service_price(&Symbol::new(&env, "never_set")),
        0i128
    );
}

#[test]
fn test_compute_billing_basic() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);
    assert_eq!(client.compute_billing(&agent, &svc), 420i128);
}

#[test]
fn test_settle_drains_usage_and_returns_billed() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);
    let billed = client.settle(&admin, &agent, &svc);
    assert_eq!(billed, 420i128);
    assert_eq!(client.get_usage(&agent, &svc), 0);
}

#[test]
fn test_pause_admin_can_pause() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
}

#[test]
fn test_unpause_admin_can_unpause() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    client.unpause();
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_settle_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.settle(&admin, &agent, &Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_usage_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &1u32);
}

#[test]
fn test_propose_admin_transfer_persists_pending() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
}

#[test]
fn test_accept_admin_transfer_rotates_admin() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    client.accept_admin_transfer(&next);
    assert_eq!(client.get_admin(), Some(next));
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_accept_admin_transfer_panics_with_no_pending() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let caller = Address::generate(&env);
    client.accept_admin_transfer(&caller);
}

#[test]
fn test_is_paused_round_trip() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_settle_returns_zero_for_unused_pair() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    assert_eq!(client.settle(&admin, &agent, &svc), 0i128);
}

#[test]
fn test_compute_billing_zero_when_unpriced_or_unused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &0u32);
}

#[test]
fn test_bool_flag_accessor_round_trip() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    let caller = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.transfer_service_ownership(&caller, &svc, &new_owner);
}

#[test]
fn test_clear_service_metadata_removes_entry() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    // Clearing a never-set entry is a no-op (no panic).
    client.clear_service_metadata(&svc);
    assert!(client.get_service_metadata(&svc).is_none());
}

#[test]
fn test_clear_service_metadata_leaves_registration_untouched() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    assert_eq!(client.get_pending_admin(), Some(next));
}

#[test]
fn test_accept_admin_transfer_clears_pending() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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

    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);

    // No settlement has happened yet for this pair.
    assert_eq!(client.get_last_settlement(&agent, &svc), None);

    let billed = client.settle(&admin, &agent, &svc);

    assert_eq!(billed, 420i128);
    // Usage drains to exactly zero.
    assert_eq!(client.get_usage(&agent, &svc), 0);
    // LastSettlement is stamped with the current ledger timestamp.
    assert_eq!(client.get_last_settlement(&agent, &svc), Some(ts));
}

#[test]
fn test_settle_billed_matches_compute_billing_for_presettle_state() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &7i128);
    client.record_usage(&agent, &svc, &13u32);

    // Capture the bill the contract would report for the pre-settle state.
    let expected = client.compute_billing(&agent, &svc);
    assert_eq!(expected, 91i128);

    let billed = client.settle(&admin, &agent, &svc);
    assert_eq!(billed, expected);
    // And compute_billing now reads zero since usage drained.
    assert_eq!(client.compute_billing(&agent, &svc), 0i128);
}

#[test]
fn test_settle_emits_settled_event_with_payload() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);

    let billed = client.settle(&admin, &agent, &svc);

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
    let (client, admin) = setup_initialized(&env);
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

    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);

    // Settle a pair that never recorded any usage.
    let billed = client.settle(&admin, &agent, &svc);
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
    let (client, admin) = setup_initialized(&env);
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_migrate_v1_to_v2_rejected_on_fresh_v2_init() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.migrate_v1_to_v2();
}

#[test]
fn test_set_service_metadata_round_trips_description_and_owner() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    assert_eq!(client.get_service_metadata(&svc), None);
}

#[test]
fn test_register_service_does_not_set_disabled_flag() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");

    client.register_service(&svc);

    assert!(client.is_service_registered(&svc));
    // Registering must not implicitly disable the service.
    assert!(!client.is_service_disabled(&svc));
}

#[test]
fn test_disable_preserves_registration_and_metadata() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);

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
    let (client, admin) = setup_initialized(&env);
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
    let (client, admin) = setup_initialized(&env);

    client.pause();
    assert!(client.is_paused());
    // Pausing an already-paused contract keeps it paused.
    client.pause();
    assert!(client.is_paused());
}

#[test]
fn test_double_unpause_is_idempotent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    // Unpausing a never-paused contract is a no-op and stays unpaused.
    client.unpause();
    assert!(!client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_pause_unpause_ends_unpaused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    client.pause();
    client.pause();
    client.unpause();

    assert!(!client.is_paused());
}

#[test]
fn test_per_pair_usage_saturates_at_u32_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");

    // record_usage takes a u32 delta, so reach the boundary across two calls:
    // (u32::MAX - 1) then 5 more would overflow -> must clamp at u32::MAX.
    client.record_usage(&agent, &service_id, &(u32::MAX - 1));
    let record = client.record_usage(&agent, &service_id, &5u32);

    assert_eq!(record.requests, u32::MAX);
    assert_eq!(client.get_usage(&agent, &service_id), u32::MAX);
}

#[test]
fn test_total_usage_by_agent_saturates_at_u32_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    // Two distinct services so the per-pair counters do not themselves clamp
    // before the per-agent lifetime counter does.
    let svc_a = Symbol::new(&env, "svc_a");
    let svc_b = Symbol::new(&env, "svc_b");

    client.record_usage(&agent, &svc_a, &(u32::MAX - 1));
    client.record_usage(&agent, &svc_b, &10u32);

    assert_eq!(client.get_total_usage_by_agent(&agent), u32::MAX);
}

#[test]
fn test_compute_billing_saturates_at_i128_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let _ = &admin;

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "expensive");

    // Huge price; any positive usage makes requests*price overflow i128.
    client.set_service_price(&service_id, &i128::MAX);
    client.record_usage(&agent, &service_id, &2u32);

    assert_eq!(client.compute_billing(&agent, &service_id), i128::MAX);
}

#[test]
fn test_compute_billing_zero_price_is_zero() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "free_api");

    // Zero price (free service): any usage bills to zero.
    client.set_service_price(&service_id, &0i128);
    client.record_usage(&agent, &service_id, &1000u32);

    assert_eq!(client.compute_billing(&agent, &service_id), 0);
}

#[test]
fn test_settle_unused_pair_returns_zero() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "never_used");

    // No usage recorded and no price set: settle bills zero.
    assert_eq!(client.settle(&admin, &agent, &service_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_service_price_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    client.set_service_price(&Symbol::new(&env, "infer"), &500i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_register_service_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    client.register_service(&Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_agent_allowed_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.set_agent_allowed(&agent, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_service_metadata_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let owner = Address::generate(&env);
    client.set_service_metadata(
        &Symbol::new(&env, "infer"),
        &String::from_str(&env, "desc"),
        &owner,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_clear_service_metadata_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    client.clear_service_metadata(&Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_max_requests_per_call_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    client.set_max_requests_per_call(&10u32);
}

#[test]
fn test_unpause_works_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    assert!(client.is_paused());
    // Lifecycle control must remain callable during an incident.
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_getter_works_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    client.pause();
    // Read getters must remain callable while paused.
    assert_eq!(client.get_service_price(&svc), 500i128);
}

#[test]
fn test_register_service_with_metadata_sets_flag_and_metadata() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");

    client.register_service_with_metadata(&svc, &description, &owner);

    // A single call sets both the registration flag and the metadata.
    assert!(client.is_service_registered(&svc));
    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, description);
    assert_eq!(meta.owner, owner);
}

#[test]
fn test_register_service_with_metadata_emits_svc_reg_event() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");

    client.register_service_with_metadata(&svc, &description, &owner);

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("svc_reg"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    let decoded: (Symbol, Address) = data.into_val(&env);
    assert_eq!(decoded, (svc.clone(), owner.clone()));
}

#[test]
fn test_register_service_with_metadata_overwrites_idempotently() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner1 = Address::generate(&env);
    let owner2 = Address::generate(&env);
    let desc1 = String::from_str(&env, "first");
    let desc2 = String::from_str(&env, "second");

    client.register_service_with_metadata(&svc, &desc1, &owner1);
    // Re-registering the same id overwrites the metadata and keeps it registered.
    client.register_service_with_metadata(&svc, &desc2, &owner2);

    assert!(client.is_service_registered(&svc));
    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, desc2);
    assert_eq!(meta.owner, owner2);
}

#[test]
fn test_register_service_with_metadata_allows_empty_description() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let empty = String::from_str(&env, "");

    client.register_service_with_metadata(&svc, &empty, &owner);

    assert!(client.is_service_registered(&svc));
    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.description, empty);
    assert_eq!(meta.owner, owner);
}

#[test]
#[should_panic]
fn test_register_service_with_metadata_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.init(&admin);

    // Clear all authorizations so the admin.require_auth() inside the
    // entrypoint has nothing to satisfy it and the call is rejected.
    let svc = Symbol::new(&env, "infer");
    let owner = Address::generate(&env);
    let description = String::from_str(&env, "GPU inference endpoint");
    env.set_auths(&[]);
    client.register_service_with_metadata(&svc, &description, &owner);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_service_price_panics_not_initialized_before_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    // No init() call: require_admin must still panic NotInitialized (#3).
    client.set_service_price(&Symbol::new(&env, "infer"), &500i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_usage_paused_gate_via_helper() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    // ensure_not_paused must still panic ContractPaused (#4) while paused.
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &1u32);
}

#[test]
fn test_get_usage_batch_preserves_order() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let svc_a = Symbol::new(&env, "svc_a");
    let svc_b = Symbol::new(&env, "svc_b");
    let svc_c = Symbol::new(&env, "svc_c");

    client.record_usage(&agent, &svc_a, &10u32);
    client.record_usage(&agent, &svc_b, &20u32);
    client.record_usage(&agent, &svc_c, &30u32);

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    pairs.push_back((agent.clone(), svc_b.clone()));
    pairs.push_back((agent.clone(), svc_a.clone()));
    pairs.push_back((agent.clone(), svc_c.clone()));

    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.len(), 3);
    assert_eq!(out.get(0), Some(20));
    assert_eq!(out.get(1), Some(10));
    assert_eq!(out.get(2), Some(30));
}

#[test]
fn test_get_usage_batch_unknown_pairs_return_zero() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "never_used");

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    pairs.push_back((agent.clone(), svc.clone()));

    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.len(), 1);
    assert_eq!(out.get(0), Some(0));
}

#[test]
fn test_get_usage_batch_mix_known_and_unknown() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let known = Symbol::new(&env, "known");
    let unknown = Symbol::new(&env, "unknown");

    client.record_usage(&agent, &known, &7u32);

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    pairs.push_back((agent.clone(), unknown.clone()));
    pairs.push_back((agent.clone(), known.clone()));

    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.get(0), Some(0));
    assert_eq!(out.get(1), Some(7));
}

#[test]
fn test_get_usage_batch_duplicate_pairs() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "dup_svc");
    client.record_usage(&agent, &svc, &42u32);

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    pairs.push_back((agent.clone(), svc.clone()));
    pairs.push_back((agent.clone(), svc.clone()));
    pairs.push_back((agent.clone(), svc.clone()));

    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.len(), 3);
    assert_eq!(out.get(0), Some(42));
    assert_eq!(out.get(1), Some(42));
    assert_eq!(out.get(2), Some(42));
}

#[test]
fn test_get_usage_batch_empty_returns_empty() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.len(), 0);
}

#[test]
fn test_get_usage_batch_at_bound_succeeds() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "bound_svc");
    client.record_usage(&agent, &svc, &5u32);

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    for _ in 0..MAX_BATCH_READ {
        pairs.push_back((agent.clone(), svc.clone()));
    }
    assert_eq!(pairs.len(), MAX_BATCH_READ);

    let out = client.get_usage_batch(&pairs);
    assert_eq!(out.len(), MAX_BATCH_READ);
    assert_eq!(out.get(0), Some(5));
    assert_eq!(out.get(MAX_BATCH_READ - 1), Some(5));
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_get_usage_batch_oversized_panics() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "over_svc");

    let mut pairs: Vec<(Address, Symbol)> = Vec::new(&env);
    for _ in 0..(MAX_BATCH_READ + 1) {
        pairs.push_back((agent.clone(), svc.clone()));
    }
    assert_eq!(pairs.len(), MAX_BATCH_READ + 1);

    client.get_usage_batch(&pairs);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_usage_paused_beats_zero_requests() {
    // Paused (#4) must win even when requests == 0 (which would be #2).
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &0u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_record_usage_zero_requests_beats_max() {
    // Zero-requests (#2) must win over the max cap (#8): with max=5 and
    // requests=0, the zero check fires first.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&5u32);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &0u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_record_usage_max_beats_min() {
    // Max (#8) must win over min (#9): with max=5 and min=10 (an
    // inconsistent config), a request above max trips #8 first.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&5u32);
    client.set_min_requests_per_call(&10u32);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &6u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_record_usage_min_beats_registration() {
    // Min (#9) must win over the registration gate (#7): with min=10 and
    // strict registration required (service unregistered), a below-min
    // request trips #9 before #7.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &3u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_record_usage_registration_beats_disabled() {
    // Registration (#7) must win over disabled (#12): require registration,
    // leave the service unregistered, and also disable it. #7 fires first.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let service_id = Symbol::new(&env, "weather_api");
    client.set_service_disabled(&service_id, &true);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &service_id, &5u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_record_usage_disabled_beats_allowlist() {
    // Disabled (#12) must win over the allowlist (#10): disable a registered
    // service and enable a (non-matching) allowlist. #12 fires first.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.register_service(&service_id);
    client.set_service_disabled(&service_id, &true);
    client.set_allowlist_enabled(&true);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &service_id, &5u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_record_usage_allowlist_fires_when_enabled_and_not_allowed() {
    // Allowlist (#10) fires when enabled and the agent is not allowed.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_allowlist_enabled(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &5u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_record_usage_registration_fires_when_required_and_unregistered() {
    // Registration (#7) fires when required and the service is unregistered.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &5u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_record_usage_disabled_fires_when_service_disabled() {
    // Disabled (#12) fires when the service is disabled.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.set_service_disabled(&service_id, &true);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &service_id, &5u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_record_usage_max_fires_above_cap() {
    // Max (#8) fires when requests exceed the configured cap.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&5u32);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &6u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_record_usage_min_fires_below_floor() {
    // Min (#9) fires when requests fall below the configured floor.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &3u32);
}

#[test]
fn test_record_usage_passes_all_gates_when_satisfied() {
    // Sanity: with every gate enabled and satisfied, record_usage succeeds.
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let service_id = Symbol::new(&env, "weather_api");
    let agent = Address::generate(&env);
    client.set_max_requests_per_call(&100u32);
    client.set_min_requests_per_call(&1u32);
    client.set_require_service_registration(&true);
    client.register_service(&service_id);
    client.set_allowlist_enabled(&true);
    client.set_agent_allowed(&agent, &true);

    let record = client.record_usage(&agent, &service_id, &5u32);
    assert_eq!(record.requests, 5);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_record_usage_rejects_blocked_agent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    client.set_agent_blocked(&agent, &true);
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_blocklist_takes_precedence_over_allowlist() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    // Enable the allowlist and explicitly allow the agent...
    client.set_allowlist_enabled(&true);
    client.set_agent_allowed(&agent, &true);
    // ...but also block it: the block must win.
    client.set_agent_blocked(&agent, &true);
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_blocked_agent_rejected_while_allowlist_disabled() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    // Allowlist stays disabled (its default); the block alone rejects.
    assert!(!client.is_allowlist_enabled());
    client.set_agent_blocked(&agent, &true);
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
fn test_unblock_then_record_succeeds() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    client.set_agent_blocked(&agent, &true);
    client.set_agent_blocked(&agent, &false);

    let record = client.record_usage(&agent, &svc, &5u32);
    assert_eq!(record.requests, 5);
    assert_eq!(client.get_usage(&agent, &svc), 5);
}

#[test]
fn test_is_agent_blocked_round_trip() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);

    // Defaults to false when never set.
    assert!(!client.is_agent_blocked(&agent));
    client.set_agent_blocked(&agent, &true);
    assert!(client.is_agent_blocked(&agent));
    client.set_agent_blocked(&agent, &false);
    assert!(!client.is_agent_blocked(&agent));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_set_agent_blocked_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.init(&admin);

    // Drop the mocked auths so the admin require_auth is enforced.
    env.set_auths(&[]);
    let agent = Address::generate(&env);
    client.set_agent_blocked(&agent, &true);
}

#[test]
fn test_remove_service_price_clears_price() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    assert_eq!(client.get_service_price(&svc), 500i128);

    client.remove_service_price(&svc);

    // Reads back 0, same as a never-priced service.
    assert_eq!(client.get_service_price(&svc), 0i128);
}

#[test]
fn test_remove_service_price_is_idempotent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    // Removing the price of a never-priced service is a no-op (no panic).
    client.remove_service_price(&svc);
    assert_eq!(client.get_service_price(&svc), 0i128);
}

#[test]
fn test_remove_service_price_then_reset_works() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    client.remove_service_price(&svc);
    assert_eq!(client.get_service_price(&svc), 0i128);

    // Re-setting after removal works and round-trips.
    client.set_service_price(&svc, &750i128);
    assert_eq!(client.get_service_price(&svc), 750i128);
}

#[test]
fn test_compute_billing_zero_after_price_removed() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &42u32);
    assert_eq!(client.compute_billing(&agent, &svc), 420i128);

    client.remove_service_price(&svc);

    // Usage is untouched, but with no price the bill is zero.
    assert_eq!(client.compute_billing(&agent, &svc), 0i128);
}

#[test]
fn test_remove_service_price_emits_price_rm_event() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);

    client.remove_service_price(&svc);

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();
    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("price_rm"),).into_val(&env);
    assert_eq!(topics, expected_topics);
    let decoded: Symbol = data.into_val(&env);
    assert_eq!(decoded, svc);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_remove_service_price_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    client.pause();
    client.remove_service_price(&svc);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_remove_service_price_non_admin_panics() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    // Drop the mocked auths so the admin's require_auth() is unsatisfied,
    // simulating a caller without the admin signature.
    env.set_auths(&[]);
    client.remove_service_price(&svc);
}

#[test]
fn test_i17_per_call_bounds_default_to_unbounded() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    // No cap and no floor configured by default.
    assert_eq!(client.get_max_requests_per_call(), u32::MAX);
    assert_eq!(client.get_min_requests_per_call(), 0);
    // Any positive value is therefore accepted.
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    assert_eq!(
        client.record_usage(&agent, &svc, &1_000_000u32).requests,
        1_000_000
    );
}

#[test]
fn test_i17_record_usage_accepts_value_exactly_at_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&100u32);
    assert_eq!(client.get_max_requests_per_call(), 100);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    // Exactly at the ceiling is allowed (boundary is inclusive).
    assert_eq!(client.record_usage(&agent, &svc, &100u32).requests, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_i17_record_usage_rejects_above_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&100u32);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &101u32);
}

#[test]
fn test_i17_record_usage_accepts_value_exactly_at_min() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    assert_eq!(client.get_min_requests_per_call(), 10);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    // Exactly at the floor is allowed (boundary is inclusive).
    assert_eq!(client.record_usage(&agent, &svc, &10u32).requests, 10);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_i17_record_usage_rejects_below_min() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &9u32);
}

#[test]
fn test_i18_strict_off_allows_unknown_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    // Default: strict registration is off, so unknown services are accepted.
    assert!(!client.is_service_registration_required());
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "unknown");
    assert_eq!(client.record_usage(&agent, &svc, &1u32).requests, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_i18_strict_on_rejects_unregistered() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    assert!(client.is_service_registration_required());
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "ghost"), &1u32);
}

#[test]
fn test_i18_register_admits_service_under_strict_mode() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.register_service(&svc);
    assert!(client.is_service_registered(&svc));
    assert_eq!(client.record_usage(&agent, &svc, &2u32).requests, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_i18_unregister_reinstates_rejection() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.register_service(&svc);
    client.unregister_service(&svc);
    assert!(!client.is_service_registered(&svc));
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_i18_disabled_service_rejects_usage() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_disabled(&svc, &true);
    assert!(client.is_service_disabled(&svc));
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
fn test_i18_reenable_service_resumes_usage() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    // Disabling then re-enabling restores the ability to accrue usage and
    // leaves the registration flag independent of the disabled flag.
    client.register_service(&svc);
    client.set_service_disabled(&svc, &true);
    client.set_service_disabled(&svc, &false);
    assert!(!client.is_service_disabled(&svc));
    assert!(client.is_service_registered(&svc));
    assert_eq!(client.record_usage(&agent, &svc, &3u32).requests, 3);
}

#[test]
fn test_i19_total_usage_by_agent_accumulates_across_services() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let a = Symbol::new(&env, "svc_a");
    let b = Symbol::new(&env, "svc_b");
    client.record_usage(&agent, &a, &5u32);
    client.record_usage(&agent, &b, &7u32);
    // Cross-service lifetime counter sums both services for the agent.
    assert_eq!(client.get_total_usage_by_agent(&agent), 12);
}

#[test]
fn test_i19_total_requests_all_time_sums_across_agents() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.record_usage(&a1, &svc, &4u32);
    client.record_usage(&a2, &svc, &6u32);
    assert_eq!(client.get_total_requests_all_time(), 10u64);
}

#[test]
fn test_i19_lifetime_counters_survive_settle() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &2i128);
    client.record_usage(&agent, &svc, &9u32);
    client.settle(&admin, &agent, &svc);
    // Per-pair usage drains, lifetime analytics persist.
    assert_eq!(client.get_usage(&agent, &svc), 0);
    assert_eq!(client.get_total_usage_by_agent(&agent), 9);
    assert_eq!(client.get_total_requests_all_time(), 9u64);
    // Re-recording after settle continues to grow the lifetime counter.
    client.record_usage(&agent, &svc, &1u32);
    assert_eq!(client.get_total_usage_by_agent(&agent), 10);
}

#[test]
fn test_i19_last_settlement_none_before_some_after() {
    let env = Env::default();
    let ts: u64 = 777;
    env.ledger().with_mut(|li| li.timestamp = ts);
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &1i128);
    client.record_usage(&agent, &svc, &3u32);
    // Never-settled reads as None (distinct from Some(0)).
    assert_eq!(client.get_last_settlement(&agent, &svc), None);
    client.settle(&admin, &agent, &svc);
    assert_eq!(client.get_last_settlement(&agent, &svc), Some(ts));
}

#[test]
fn test_i19_last_settlement_is_none_for_never_settled_pair() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "never");
    assert_eq!(client.get_last_settlement(&agent, &svc), None);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_i20_cancel_then_accept_fails() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    client.cancel_admin_transfer();
    // Nothing pending after a cancel, so accept must fail with #5.
    client.accept_admin_transfer(&next);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_i20_wrong_caller_accept_rejected() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    let intruder = Address::generate(&env);
    client.propose_admin_transfer(&next);
    // A caller other than the pending admin is rejected with #6.
    client.accept_admin_transfer(&intruder);
}

#[test]
fn test_i20_repropose_overwrites_pending() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.propose_admin_transfer(&first);
    assert_eq!(client.get_pending_admin(), Some(first));
    client.propose_admin_transfer(&second);
    assert_eq!(client.get_pending_admin(), Some(second.clone()));
    // Only the most recent pending admin can accept.
    client.accept_admin_transfer(&second);
    assert_eq!(client.get_admin(), Some(second));
}

#[test]
fn test_i20_rotated_admin_can_act_after_handover() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
    client.accept_admin_transfer(&next);
    // The rotated admin can now perform an admin-gated action.
    client.pause();
    assert!(client.is_paused());
}

#[test]
fn test_i20_schema_version_is_two_after_init() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    // Fresh v2 init stamps SchemaVersion = 2 directly (no migration needed).
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_i20_double_migrate_guard_rejects_on_v2() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    // Already at v2, so the v1->v2 migration refuses with #11.
    client.migrate_v1_to_v2();
}

#[test]
fn test_i21_per_pair_usage_saturates_at_u32_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.record_usage(&agent, &svc, &u32::MAX);
    // Adding more saturates at u32::MAX rather than overflowing.
    assert_eq!(client.record_usage(&agent, &svc, &10u32).requests, u32::MAX);
    assert_eq!(client.get_usage(&agent, &svc), u32::MAX);
}

#[test]
fn test_i21_total_usage_by_agent_saturates() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let a = Symbol::new(&env, "svc_a");
    let b = Symbol::new(&env, "svc_b");
    client.record_usage(&agent, &a, &u32::MAX);
    client.record_usage(&agent, &b, &u32::MAX);
    // The cross-service lifetime counter also saturates at u32::MAX.
    assert_eq!(client.get_total_usage_by_agent(&agent), u32::MAX);
}

#[test]
fn test_i21_compute_billing_saturates_at_i128_max() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &i128::MAX);
    client.record_usage(&agent, &svc, &2u32);
    // 2 * i128::MAX saturates to i128::MAX rather than overflowing.
    assert_eq!(client.compute_billing(&agent, &svc), i128::MAX);
}

#[test]
fn test_i21_settle_returns_saturated_value_and_drains() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &i128::MAX);
    client.record_usage(&agent, &svc, &5u32);
    let billed = client.settle(&admin, &agent, &svc);
    assert_eq!(billed, i128::MAX);
    // The counter still drains to zero even when billing saturated.
    assert_eq!(client.get_usage(&agent, &svc), 0);
}

#[test]
fn test_i21_total_requests_all_time_accumulates_large_values() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    // u64 protocol counter comfortably sums two u32::MAX increments.
    client.record_usage(&a1, &svc, &u32::MAX);
    client.record_usage(&a2, &svc, &u32::MAX);
    assert_eq!(client.get_total_requests_all_time(), (u32::MAX as u64) * 2);
}

/// Register and `init` the contract authorising only `admin` for the `init`
/// call. Subsequent privileged calls are intentionally left unauthorised so
/// their `require_auth` fails.
fn setup_scoped_auth(env: &Env) -> EscrowClient<'_> {
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "init",
            args: (admin.clone(),).into_val(env),
            sub_invokes: &[],
        },
    }]);
    client.init(&admin);
    client
}

#[test]
#[should_panic]
fn test_i22_pause_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    client.pause();
}

#[test]
#[should_panic]
fn test_i22_set_service_price_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    client.set_service_price(&Symbol::new(&env, "infer"), &10i128);
}

#[test]
#[should_panic]
fn test_i22_register_service_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    client.register_service(&Symbol::new(&env, "infer"));
}

#[test]
#[should_panic]
fn test_i22_set_agent_allowed_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    let agent = Address::generate(&env);
    client.set_agent_allowed(&agent, &true);
}

#[test]
#[should_panic]
fn test_i22_set_service_disabled_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    client.set_service_disabled(&Symbol::new(&env, "infer"), &true);
}

#[test]
#[should_panic]
fn test_i22_migrate_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    client.migrate_v1_to_v2();
}

#[test]
#[should_panic]
fn test_i22_propose_admin_transfer_requires_admin_auth() {
    let env = Env::default();
    let client = setup_scoped_auth(&env);
    let next = Address::generate(&env);
    client.propose_admin_transfer(&next);
}

/// Positive control: with `mock_all_auths` the same privileged call
/// succeeds, proving the panics above stem from the missing signature.
#[test]
fn test_i22_pause_succeeds_with_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    client.pause();
    assert!(client.is_paused());
}

/// With the allowlist disabled (the default), any agent can record usage.
#[test]
fn test_allowlist_disabled_allows_any_agent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    assert!(!client.is_allowlist_enabled());

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    let record = client.record_usage(&agent, &service_id, &5u32);
    assert_eq!(record.requests, 5);
}

/// With the allowlist enabled and the agent not listed, record_usage panics
/// with AgentNotAllowed (#10).
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_allowlist_enabled_rejects_unlisted_agent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_allowlist_enabled(&true);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &1u32);
}

/// With the allowlist enabled and the agent explicitly allowed, record_usage
/// succeeds.
#[test]
fn test_allowlist_enabled_allows_listed_agent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_allowlist_enabled(&true);

    let agent = Address::generate(&env);
    client.set_agent_allowed(&agent, &true);
    assert!(client.is_agent_allowed(&agent));

    let service_id = Symbol::new(&env, "weather_api");
    let record = client.record_usage(&agent, &service_id, &3u32);
    assert_eq!(record.requests, 3);
}

/// An agent allowed then revoked is rejected again with #10.
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_allowlist_revocation_reblocks_agent() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_allowlist_enabled(&true);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.set_agent_allowed(&agent, &true);
    client.record_usage(&agent, &service_id, &2u32);

    // Revoke and try again — must be rejected.
    client.set_agent_allowed(&agent, &false);
    assert!(!client.is_agent_allowed(&agent));
    client.record_usage(&agent, &service_id, &1u32);
}

/// Disabling the gate after enabling it restores access for any agent.
#[test]
fn test_allowlist_disable_restores_access() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");

    client.set_allowlist_enabled(&true);
    // Gate on, agent unlisted → blocked (try_ to avoid unwinding the test).
    assert!(client.try_record_usage(&agent, &service_id, &1u32).is_err());

    // Turn the gate back off; the unlisted agent can record again.
    client.set_allowlist_enabled(&false);
    let record = client.record_usage(&agent, &service_id, &7u32);
    assert_eq!(record.requests, 7);
}

/// is_allowlist_enabled / is_agent_allowed round-trip cleanly.
#[test]
fn test_allowlist_status_round_trips() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);

    assert!(!client.is_allowlist_enabled());
    client.set_allowlist_enabled(&true);
    assert!(client.is_allowlist_enabled());

    let agent = Address::generate(&env);
    assert!(!client.is_agent_allowed(&agent));
    client.set_agent_allowed(&agent, &true);
    assert!(client.is_agent_allowed(&agent));
    client.set_agent_allowed(&agent, &false);
    assert!(!client.is_agent_allowed(&agent));
}

/// With the gate on, multiple agents of mixed status are handled independently:
/// the allowed one records, the unlisted one is blocked.
#[test]
fn test_allowlist_mixed_agents() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let service_id = Symbol::new(&env, "weather_api");

    let allowed = Address::generate(&env);
    let blocked = Address::generate(&env);
    client.set_allowlist_enabled(&true);
    client.set_agent_allowed(&allowed, &true);

    let record = client.record_usage(&allowed, &service_id, &4u32);
    assert_eq!(record.requests, 4);
    assert!(client
        .try_record_usage(&blocked, &service_id, &1u32)
        .is_err());
}

/// With strict registration off (default), pricing an unregistered service
/// still works — backward compatible.
#[test]
fn test_set_price_lax_allows_unregistered_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    assert_eq!(client.get_service_price(&svc), 500i128);
}

/// With strict registration on, pricing a registered service works.
#[test]
fn test_set_price_strict_allows_registered_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_require_service_registration(&true);
    client.register_service(&svc);
    client.set_service_price(&svc, &750i128);
    assert_eq!(client.get_service_price(&svc), 750i128);
}

/// With strict registration on, pricing an unregistered service is rejected
/// with ServiceNotRegistered (#7).
#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_set_price_strict_rejects_unregistered_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "phantom");
    client.set_require_service_registration(&true);
    client.set_service_price(&svc, &100i128);
}

/// Pricing a disabled service is rejected with ServiceDisabled (#12),
/// regardless of the strict-registration flag.
#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_set_price_rejects_disabled_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_disabled(&svc, &true);
    client.set_service_price(&svc, &100i128);
}

/// Toggling the flag on mid-life starts enforcing the coupling: a service
/// priced while lax can no longer be re-priced once strict unless registered.
#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_set_price_flag_toggled_mid_life() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &100i128); // lax: allowed
    client.set_require_service_registration(&true);
    client.set_service_price(&svc, &200i128); // strict + unregistered: rejected
}

/// The registered service owner can settle their own service without the
/// admin key.
#[test]
fn test_owner_can_settle_own_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    client.set_service_metadata(&svc, &String::from_str(&env, "inference"), &owner);
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &5u32);

    let billed = client.settle(&owner, &agent, &svc);
    assert_eq!(billed, 50i128);
    assert_eq!(client.get_usage(&agent, &svc), 0);
}

/// The admin can always settle, even a service owned by someone else.
#[test]
fn test_admin_can_settle_owned_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    client.set_service_metadata(&svc, &String::from_str(&env, "inference"), &owner);
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &4u32);

    let billed = client.settle(&admin, &agent, &svc);
    assert_eq!(billed, 40i128);
}

/// The owner of service A cannot settle service B (panics #6, the reused
/// unauthorized-caller error).
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_owner_cannot_settle_other_service() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);
    let agent = Address::generate(&env);
    let svc_a = Symbol::new(&env, "svc_a");
    let svc_b = Symbol::new(&env, "svc_b");

    client.set_service_metadata(&svc_a, &String::from_str(&env, "a"), &owner_a);
    client.set_service_metadata(&svc_b, &String::from_str(&env, "b"), &owner_b);
    client.set_service_price(&svc_b, &10i128);
    client.record_usage(&agent, &svc_b, &3u32);

    // owner_a tries to settle svc_b — unauthorized.
    client.settle(&owner_a, &agent, &svc_b);
}

/// A non-admin caller settling a service with no metadata is rejected with
/// ServiceMetadataNotFound (#13).
#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_nonadmin_settle_without_metadata_rejected() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let stranger = Address::generate(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &10i128);
    client.record_usage(&agent, &svc, &2u32);

    client.settle(&stranger, &agent, &svc);
}

/// The pause gate still applies to owner-authorized settlement.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_owner_settle_rejected_while_paused() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_metadata(&svc, &String::from_str(&env, "inference"), &owner);
    client.pause();
    client.settle(&owner, &agent, &svc);
}

/// By default the limiter is disabled (cap 0, window 0): an agent can record
/// far more than any cap would allow.
#[test]
fn test_rate_limit_disabled_by_default() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    assert_eq!(client.get_max_requests_per_window(), 0);
    assert_eq!(client.get_rate_window_seconds(), 0);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    for _ in 0..50 {
        client.record_usage(&agent, &svc, &100u32);
    }
    assert_eq!(client.get_usage(&agent, &svc), 5_000);
}

/// Config setters round-trip.
#[test]
fn test_rate_limit_config_round_trips() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_window(&10u32);
    client.set_rate_window_seconds(&60u64);
    assert_eq!(client.get_max_requests_per_window(), 10);
    assert_eq!(client.get_rate_window_seconds(), 60);
}

/// Accumulating exactly up to the cap is allowed; one more request in the
/// same window is rejected with RateLimitExceeded (#15).
#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_rate_limit_rejects_over_cap_in_window() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_window(&10u32);
    client.set_rate_window_seconds(&100u64);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.record_usage(&agent, &svc, &6u32); // count = 6
    client.record_usage(&agent, &svc, &4u32); // count = 10 (exactly at cap)
    client.record_usage(&agent, &svc, &1u32); // count = 11 → reject #15
}

/// After the window expires the counter resets and the agent can record
/// again (fixed-window rollover).
#[test]
fn test_rate_limit_window_rollover_resets_count() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_window(&10u32);
    client.set_rate_window_seconds(&100u64);

    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.record_usage(&agent, &svc, &10u32); // fills the window

    // Advance past the window; the count resets.
    env.ledger().with_mut(|li| li.timestamp = 1_100);
    let rec = client.record_usage(&agent, &svc, &10u32);
    // Usage is cumulative (20), but the rate window accepted the new 10.
    assert_eq!(rec.requests, 20);
}

/// The limiter is per-agent: one agent hitting the cap does not block another.
#[test]
fn test_rate_limit_is_per_agent() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin) = setup_initialized(&env);
    client.set_max_requests_per_window(&5u32);
    client.set_rate_window_seconds(&100u64);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.record_usage(&a, &svc, &5u32); // a at cap
    let rec_b = client.record_usage(&b, &svc, &5u32); // b independent
    assert_eq!(rec_b.requests, 5);
}
// ── compute_billing tests ────────────────────────────────────────────────────
//
// `compute_billing(agent, service_id)` returns `accumulated_requests * price_per_request`
// using `saturating_mul`, returns `0` when either operand is zero, and is the
// read-only mirror of the billing math inside `settle`.
//
// Covered scenarios:
//   1. Zero usage, any price          → 0
//   2. Zero price (free service)      → 0
//   3. Unpriced and unused pair       → 0
//   4. Normal product                 → requests * price
//   5. Saturation edge                → i128::MAX (no overflow)
//   6. compute_billing agrees with settle billed value

/// Helper: register a service price for `service_id`.
fn set_price(client: &EscrowClient, service_id: &Symbol, price: i128) {
    client.set_service_price(service_id, &price);
}

/// Helper: record `requests` units of usage for `(agent, service_id)`.
fn record(client: &EscrowClient, agent: &Address, service_id: &Symbol, requests: u32) {
    client.record_usage(agent, service_id, &requests);
}

/// Zero usage with a non-zero price must bill 0.
#[test]
fn test_compute_billing_zero_usage() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    set_price(&client, &svc, 100);
    // No record_usage call — accumulated_requests is 0.
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(bill, 0, "zero usage must bill 0 regardless of price");
}

/// Zero price (free service) with non-zero usage must bill 0.
#[test]
fn test_compute_billing_zero_price_free_service() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "free");

    set_price(&client, &svc, 0); // explicitly free
    record(&client, &agent, &svc, 50);
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(bill, 0, "free service (price=0) must always bill 0");
}

/// Pair with no price set and no usage recorded must bill 0.
#[test]
fn test_compute_billing_unpriced_and_unused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "ghost");

    // Neither set_service_price nor record_usage called.
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(bill, 0, "unpriced and unused pair must bill 0");
}

/// Normal product: 10 requests × 250 stroops/request = 2_500 stroops.
#[test]
fn test_compute_billing_normal_product() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "embed");

    set_price(&client, &svc, 250);
    record(&client, &agent, &svc, 10);
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(bill, 2_500, "10 requests × 250 stroops must equal 2500");
}

/// Accumulated usage across multiple record_usage calls is summed correctly.
#[test]
fn test_compute_billing_accumulated_usage() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "chat");

    set_price(&client, &svc, 10);
    record(&client, &agent, &svc, 5);
    record(&client, &agent, &svc, 15);
    // total usage = 20, price = 10 → bill = 200
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(
        bill, 200,
        "accumulated usage across calls must sum correctly"
    );
}

/// Saturation edge: large requests × large price saturates at i128::MAX.
#[test]
fn test_compute_billing_saturation() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "sat");

    // i128::MAX / i128::MAX would overflow without saturating_mul.
    // Use price = i128::MAX so that even 1 request saturates.
    set_price(&client, &svc, i128::MAX);
    record(&client, &agent, &svc, 1);
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(
        bill,
        i128::MAX,
        "1 request × i128::MAX price must saturate at i128::MAX"
    );
}

/// Saturation with u32::MAX requests also saturates at i128::MAX.
#[test]
fn test_compute_billing_saturation_large_requests() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "sat2");

    // price high enough that u32::MAX * price overflows i128
    set_price(&client, &svc, i128::MAX);
    // record_usage caps at u32::MAX via saturating_add, so record in steps
    record(&client, &agent, &svc, u32::MAX);
    let bill = client.compute_billing(&agent, &svc);
    assert_eq!(
        bill,
        i128::MAX,
        "u32::MAX requests × large price must saturate at i128::MAX"
    );
}

/// compute_billing agrees with the `billed` value settle returns for the same state.
#[test]
fn test_compute_billing_agrees_with_settle() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "agree");

    set_price(&client, &svc, 75);
    record(&client, &agent, &svc, 8);

    // Read compute_billing BEFORE settle (pre-settle state).
    let pre_settle_bill = client.compute_billing(&agent, &svc);

    // settle returns the billed amount and drains the counter.
    let settled = client.settle(&admin, &agent, &svc);

    assert_eq!(
        pre_settle_bill, settled,
        "compute_billing must equal the billed value settle returns for the same pre-settle state"
    );
    assert_eq!(pre_settle_bill, 600, "8 requests × 75 stroops = 600");
}

/// After settle drains the counter, compute_billing returns 0.
#[test]
fn test_compute_billing_zero_after_settle() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "drain");

    set_price(&client, &svc, 50);
    record(&client, &agent, &svc, 4);
    client.settle(&admin, &agent, &svc);

    // Counter is drained — billing must now be 0.
    let post_settle_bill = client.compute_billing(&agent, &svc);
    assert_eq!(
        post_settle_bill, 0,
        "compute_billing must return 0 after settle drains the counter"
    );
}

/// Different agents billed independently for the same service.
#[test]
fn test_compute_billing_independent_per_agent() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let svc = Symbol::new(&env, "shared");

    set_price(&client, &svc, 20);
    record(&client, &a, &svc, 3); // a: 3 × 20 = 60
    record(&client, &b, &svc, 7); // b: 7 × 20 = 140

    assert_eq!(client.compute_billing(&a, &svc), 60);
    assert_eq!(client.compute_billing(&b, &svc), 140);
}

/// Different services billed independently for the same agent.
#[test]
fn test_compute_billing_independent_per_service() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc1 = Symbol::new(&env, "alpha");
    let svc2 = Symbol::new(&env, "beta");

    set_price(&client, &svc1, 10);
    set_price(&client, &svc2, 30);
    record(&client, &agent, &svc1, 5); // 5 × 10 = 50
    record(&client, &agent, &svc2, 2); // 2 × 30 = 60

    assert_eq!(client.compute_billing(&agent, &svc1), 50);
    assert_eq!(client.compute_billing(&agent, &svc2), 60);
}
// ── transfer_service_ownership tests ────────────────────────────────────────
//
// Auth matrix covered:
//   - Current owner transfers → allowed
//   - Admin transfers on owner's behalf → allowed
//   - Third-party caller → rejected (NotPendingAdmin)
//   - No metadata → rejected (ServiceMetadataNotFound #13)
//   - Paused contract → rejected (ContractPaused #4)
//
// Invariants verified:
//   - description is always preserved after transfer
//   - owner_chg event carries correct (service_id, old_owner, new_owner)
//   - get_service_metadata reflects new owner after transfer

/// Helper: register a service with metadata (description + owner).
fn setup_service_with_metadata<'a>(
    client: &EscrowClient<'a>,
    env: &Env,
    service_id: &Symbol,
    description: &str,
    owner: &Address,
) {
    client.register_service_with_metadata(
        service_id,
        &soroban_sdk::String::from_str(env, description),
        owner,
    );
}

/// Positive: current owner transfers to new owner — succeeds.
#[test]
fn test_transfer_ownership_by_owner_succeeds() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");

    setup_service_with_metadata(&client, &env, &svc, "Inference API", &owner);
    client.transfer_service_ownership(&owner, &svc, &new_owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.owner, new_owner);
}

/// Invariant: description is preserved after transfer.
#[test]
fn test_transfer_ownership_preserves_description() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "embed");
    let desc = "Embedding service for AgentPay";

    setup_service_with_metadata(&client, &env, &svc, desc, &owner);
    client.transfer_service_ownership(&owner, &svc, &new_owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(
        meta.description,
        soroban_sdk::String::from_str(&env, desc),
        "description must not change after ownership transfer"
    );
    assert_eq!(meta.owner, new_owner);
}

/// Positive: admin transfers on the owner's behalf — succeeds.
#[test]
fn test_transfer_ownership_by_admin_succeeds() {
    let env = Env::default();
    let (client, admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "chat");

    setup_service_with_metadata(&client, &env, &svc, "Chat API", &owner);
    // Admin calls transfer on behalf of the service owner.
    client.transfer_service_ownership(&admin, &svc, &new_owner);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.owner, new_owner);
}

/// Event: owner_chg emits (service_id, old_owner, new_owner) correctly.
#[test]
fn test_transfer_ownership_emits_owner_chg_event() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "search");

    setup_service_with_metadata(&client, &env, &svc, "Search API", &owner);
    client.transfer_service_ownership(&owner, &svc, &new_owner);

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_addr, topics, data) = events.last().unwrap();

    let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
        (symbol_short!("owner_chg"),).into_val(&env);
    assert_eq!(topics, expected_topics, "topic must be owner_chg");

    let decoded: (Symbol, Address, Address) = data.into_val(&env);
    assert_eq!(decoded.0, svc, "event service_id mismatch");
    assert_eq!(decoded.1, owner, "event old_owner mismatch");
    assert_eq!(decoded.2, new_owner, "event new_owner mismatch");
}

/// Negative: third-party caller (neither owner nor admin) is rejected.
#[test]
#[should_panic]
fn test_transfer_ownership_stranger_rejected() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "tts");

    setup_service_with_metadata(&client, &env, &svc, "TTS API", &owner);

    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "transfer_service_ownership",
            args: (stranger.clone(), svc.clone(), new_owner.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.transfer_service_ownership(&stranger, &svc, &new_owner);
}

/// Negative: transferring a service with no metadata panics #13.
#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_transfer_ownership_no_metadata_panics() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "ghost");

    // No set_service_metadata or register_service_with_metadata called.
    client.transfer_service_ownership(&owner, &svc, &new_owner);
}

/// Negative: pause gate fires (#4) before ownership transfer.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_transfer_ownership_paused_rejected() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "stt");

    setup_service_with_metadata(&client, &env, &svc, "STT API", &owner);
    client.pause();
    client.transfer_service_ownership(&owner, &svc, &new_owner);
}

/// After transfer, get_service_metadata reflects the new owner.
#[test]
fn test_transfer_ownership_metadata_updated() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let svc = Symbol::new(&env, "ocr");
    let desc = "OCR service";

    setup_service_with_metadata(&client, &env, &svc, desc, &owner);

    // Confirm initial state.
    let before = client.get_service_metadata(&svc).unwrap();
    assert_eq!(before.owner, owner);

    client.transfer_service_ownership(&owner, &svc, &new_owner);

    // Confirm updated state.
    let after = client.get_service_metadata(&svc).unwrap();
    assert_eq!(after.owner, new_owner, "owner must be updated");
    assert_eq!(
        after.description,
        soroban_sdk::String::from_str(&env, desc),
        "description must be unchanged"
    );
}

/// Chained transfer: new owner can transfer again — ownership is live.
#[test]
fn test_transfer_ownership_chained() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let second = Address::generate(&env);
    let third = Address::generate(&env);
    let svc = Symbol::new(&env, "chain");

    setup_service_with_metadata(&client, &env, &svc, "Chained", &owner);
    client.transfer_service_ownership(&owner, &svc, &second);
    client.transfer_service_ownership(&second, &svc, &third);

    let meta = client.get_service_metadata(&svc).unwrap();
    assert_eq!(meta.owner, third);
}

/// Original owner cannot transfer again after handing off.
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_ownership_old_owner_rejected_after_transfer() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let third = Address::generate(&env);
    let svc = Symbol::new(&env, "revoke");

    setup_service_with_metadata(&client, &env, &svc, "Revoke test", &owner);
    client.transfer_service_ownership(&owner, &svc, &new_owner);

    // Old owner tries to transfer again — must be rejected.
    client.transfer_service_ownership(&owner, &svc, &third);
}
