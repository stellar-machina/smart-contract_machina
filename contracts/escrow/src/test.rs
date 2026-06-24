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
    assert_eq!(
        client.get_service_price(&Symbol::new(&env, "never_set")),
        0i128
    );
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

// --- Arithmetic overflow/saturation policy (see docs/escrow/arithmetic.md) ---

#[test]
fn test_per_pair_usage_saturates_at_u32_max() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);

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
    let (client, _admin) = setup_initialized(&env);

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
    let (client, _admin) = setup_initialized(&env);

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
    let (client, _admin) = setup_initialized(&env);

    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "never_used");

    // No usage recorded and no price set: settle bills zero.
    assert_eq!(client.settle(&agent, &service_id), 0);
// --- Pause gate coverage for config-mutation entrypoints (issue #23) ---
// Every admin config mutation must respect the emergency-stop flag. These
// assert each representative entrypoint panics with ContractPaused (#4)
// once the contract is paused.

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_service_price_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    client.set_service_price(&Symbol::new(&env, "infer"), &500i128);
#[test]
fn test_remove_service_price_clears_price() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "never_set");
    // Removing the price of a never-priced service is a no-op (no panic).
    client.remove_service_price(&svc);
    assert_eq!(client.get_service_price(&svc), 0i128);
}

#[test]
fn test_remove_service_price_then_reset_works() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
fn test_register_service_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    client.register_service(&Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_agent_allowed_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    let agent = Address::generate(&env);
    client.set_agent_allowed(&agent, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_service_metadata_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    let owner = Address::generate(&env);
    client.set_service_metadata(
        &Symbol::new(&env, "infer"),
        &String::from_str(&env, "desc"),
        &owner,
// ---------------------------------------------------------------------------
// Issue #17 — per-call request floor/ceiling coverage for `record_usage`.
// Covers the default sentinels (max = u32::MAX, min = 0), exact-bound
// acceptance, and the over-max (#8) / under-min (#9) rejection paths.
// ---------------------------------------------------------------------------

#[test]
fn test_i17_per_call_bounds_default_to_unbounded() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
#[should_panic(expected = "Error(Contract, #4)")]
fn test_clear_service_metadata_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    client.clear_service_metadata(&Symbol::new(&env, "infer"));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_max_requests_per_call_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    client.set_max_requests_per_call(&10u32);
}

#[test]
fn test_unpause_works_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.pause();
    assert!(client.is_paused());
    // Lifecycle control must remain callable during an incident.
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_getter_works_while_paused() {
fn test_remove_service_price_rejected_while_paused() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    client.pause();
    // Read getters must remain callable while paused.
    assert_eq!(client.get_service_price(&svc), 500i128);
// ---- record_usage validation-chain error precedence -------------------
// These assert that the fixed error ordering
//   Paused(#4) -> ZeroRequests(#2) -> Max(#8) -> Min(#9)
//   -> Registration(#7) -> Disabled(#12) -> Allowlist(#10)
// is preserved after the read-ordering refactor, and that each gate still
// fires on its own trigger.

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_usage_paused_beats_zero_requests() {
    // Paused (#4) must win even when requests == 0 (which would be #2).
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &3u32);
// ---------------------------------------------------------------------------
// Issue #18 — strict service-registration (#7) and service-disabled (#12)
// gates in `record_usage`, plus the registry/disabled accessor round-trips.
// ---------------------------------------------------------------------------

#[test]
fn test_i18_strict_off_allows_unknown_service() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    // Default: strict registration is off, so unknown services are accepted.
    assert!(!client.is_service_registration_required());
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "unknown");
    assert_eq!(client.record_usage(&agent, &svc, &1u32).requests, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_record_usage_registration_beats_disabled() {
    // Registration (#7) must win over disabled (#12): require registration,
    // leave the service unregistered, and also disable it. #7 fires first.
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    client.set_allowlist_enabled(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &5u32);
fn test_i18_strict_on_rejects_unregistered() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    assert!(client.is_service_registration_required());
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "ghost"), &1u32);
}

#[test]
fn test_i18_register_admits_service_under_strict_mode() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.register_service(&svc);
    assert!(client.is_service_registered(&svc));
    assert_eq!(client.record_usage(&agent, &svc, &2u32).requests, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_record_usage_registration_fires_when_required_and_unregistered() {
    // Registration (#7) fires when required and the service is unregistered.
fn test_i18_unregister_reinstates_rejection() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    client.set_require_service_registration(&true);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &5u32);
    let svc = Symbol::new(&env, "infer");
    client.register_service(&svc);
    client.unregister_service(&svc);
    assert!(!client.is_service_registered(&svc));
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_record_usage_disabled_fires_when_service_disabled() {
    // Disabled (#12) fires when the service is disabled.
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    let agent = Address::generate(&env);
    let service_id = Symbol::new(&env, "weather_api");
    client.record_usage(&agent, &service_id, &3u32);
}

#[test]
fn test_record_usage_passes_all_gates_when_satisfied() {
    // Sanity: with every gate enabled and satisfied, record_usage succeeds.
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    client.remove_service_price(&svc);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_remove_service_price_non_admin_panics() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &500i128);
    // Drop the mocked auths so the admin's require_auth() is unsatisfied,
    // simulating a caller without the admin signature.
    env.set_auths(&[]);
    client.remove_service_price(&svc);
fn test_i17_record_usage_accepts_value_exactly_at_max() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    client.set_max_requests_per_call(&100u32);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &101u32);
}

#[test]
fn test_i17_record_usage_accepts_value_exactly_at_min() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    client.set_min_requests_per_call(&10u32);
    let agent = Address::generate(&env);
    client.record_usage(&agent, &Symbol::new(&env, "infer"), &9u32);
fn test_i18_disabled_service_rejects_usage() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_disabled(&svc, &true);
    assert!(client.is_service_disabled(&svc));
    client.record_usage(&agent, &svc, &1u32);
}

#[test]
fn test_i18_reenable_service_resumes_usage() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
// ---------------------------------------------------------------------------
// Issue #19 — lifetime usage counters and last-settlement timestamps.
// Locks down the invariant that `settle` drains the per-pair counter but
// never resets the lifetime analytics counters, and that LastSettlement
// distinguishes "never settled" (None) from a genesis settle (Some(0)).
// ---------------------------------------------------------------------------

#[test]
fn test_i19_total_usage_by_agent_accumulates_across_services() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &2i128);
    client.record_usage(&agent, &svc, &9u32);
    client.settle(&agent, &svc);
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
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "infer");
    client.set_service_price(&svc, &1i128);
    client.record_usage(&agent, &svc, &3u32);
    // Never-settled reads as None (distinct from Some(0)).
    assert_eq!(client.get_last_settlement(&agent, &svc), None);
    client.settle(&agent, &svc);
    assert_eq!(client.get_last_settlement(&agent, &svc), Some(ts));
}

#[test]
fn test_i19_last_settlement_is_none_for_never_settled_pair() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    let agent = Address::generate(&env);
    let svc = Symbol::new(&env, "never");
    assert_eq!(client.get_last_settlement(&agent, &svc), None);
// ---------------------------------------------------------------------------
// Issue #20 — two-step admin handover edge cases and the migration version
// guard. Covers cancel-then-accept (#5), wrong-caller accept (#6), re-propose
// overwrite, post-rotation admin authority, and the double-migrate guard (#11).
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_i20_cancel_then_accept_fails() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    let next = Address::generate(&env);
    let intruder = Address::generate(&env);
    client.propose_admin_transfer(&next);
    // A caller other than the pending admin is rejected with #6.
    client.accept_admin_transfer(&intruder);
}

#[test]
fn test_i20_repropose_overwrites_pending() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
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
    let (client, _admin) = setup_initialized(&env);
    // Fresh v2 init stamps SchemaVersion = 2 directly (no migration needed).
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_i20_double_migrate_guard_rejects_on_v2() {
    let env = Env::default();
    let (client, _admin) = setup_initialized(&env);
    // Already at v2, so the v1->v2 migration refuses with #11.
    client.migrate_v1_to_v2();
}
