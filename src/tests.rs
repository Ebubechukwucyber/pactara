use super::*;
use ed25519_dalek::{Signer, SigningKey};
use near_sdk::test_utils::{accounts, VMContextBuilder};
use near_sdk::{testing_env, PromiseResult, VMContext};
use std::collections::HashMap;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const NS_PER_DAY: u64 = 86_400_000_000_000;

fn contract_id() -> AccountId {
    accounts(0)
}

fn owner() -> AccountId {
    accounts(0)
}

fn buyer() -> AccountId {
    accounts(1)
}

fn worker() -> AccountId {
    accounts(2)
}

fn outsider() -> AccountId {
    accounts(3)
}

fn submitter() -> AccountId {
    accounts(4)
}

fn context(predecessor: AccountId, timestamp: u64) -> VMContext {
    VMContextBuilder::new()
        .current_account_id(contract_id())
        .predecessor_account_id(predecessor.clone())
        .signer_account_id(predecessor)
        .block_timestamp(timestamp)
        .build()
}

fn context_deposit(predecessor: AccountId, amount: u128, timestamp: u64) -> VMContext {
    VMContextBuilder::new()
        .current_account_id(contract_id())
        .predecessor_account_id(predecessor.clone())
        .signer_account_id(predecessor)
        .attached_deposit(NearToken::from_yoctonear(amount))
        .block_timestamp(timestamp)
        .build()
}

fn env_with_promises(ctx: VMContext, results: Vec<PromiseResult>) {
    testing_env!(
        ctx,
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        HashMap::default(),
        results,
    );
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn verifier_public_key() -> PublicKey {
    let vk = signing_key().verifying_key();
    let mut bytes = vec![0u8];
    bytes.extend_from_slice(vk.as_bytes());
    PublicKey::try_from(bytes).unwrap()
}

fn hash_n(n: u8) -> CryptoHash {
    [n; 32]
}

fn sign(
    contract: &Pactara,
    task_id: u64,
    criteria: CryptoHash,
    input: CryptoHash,
    artifact: CryptoHash,
    passed: bool,
) -> Vec<u8> {
    let message = contract.get_signing_message(task_id, criteria, input, artifact, passed);
    signing_key().sign(&message).to_bytes().to_vec()
}

fn assert_invariants(contract: &Pactara) {
    let (total, available, reserved) = contract.get_treasury();
    let pending = contract.get_pending_out();
    assert!(
        total.0 >= available.0 + reserved.0,
        "total >= available + reserved"
    );
    assert_eq!(
        total.0,
        available.0 + reserved.0 + pending.0,
        "total == available + reserved + pending_out"
    );
    let (ok, _, _, _, _) = contract.get_health();
    assert!(ok, "get_health must be true after a valid transition");
}

fn setup_at(timestamp: u64) -> Pactara {
    testing_env!(context_deposit(owner(), 10 * ONE_NEAR, timestamp));
    let mut contract = Pactara::new(owner(), U128(ONE_NEAR), U128(5 * ONE_NEAR));
    contract.deposit();
    contract.add_authorized_buyer(buyer());
    contract.add_authorized_worker(worker());
    contract.register_verifier(verifier_public_key());
    assert_invariants(&contract);
    contract
}

fn setup() -> Pactara {
    setup_at(1)
}

fn create_funded_task(contract: &mut Pactara, amount: u128, deadline: u64) -> u64 {
    testing_env!(context(buyer(), 1));
    contract.create_task(worker(), U128(amount), hash_n(1), hash_n(2), deadline)
}

fn submit_work(contract: &mut Pactara, task_id: u64) {
    testing_env!(context(worker(), 1));
    contract.submit_artifact(task_id, hash_n(3));
}

fn verify(
    contract: &mut Pactara,
    task_id: u64,
    passed: bool,
    criteria: CryptoHash,
    input: CryptoHash,
    artifact: CryptoHash,
    signature: Vec<u8>,
) {
    testing_env!(context(submitter(), 1));
    contract.submit_verification(task_id, passed, criteria, input, artifact, signature);
}

fn submitted_task() -> (Pactara, u64) {
    let mut contract = setup();
    let task_id = create_funded_task(&mut contract, 100, u64::MAX);
    submit_work(&mut contract, task_id);
    (contract, task_id)
}

#[test]
#[should_panic(expected = "Unauthorized buyer")]
fn unauthorized_buyer_rejected() {
    let mut contract = setup();
    testing_env!(context(outsider(), 1));
    contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
#[should_panic(expected = "Unauthorized worker")]
fn unauthorized_worker_rejected() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    contract.create_task(outsider(), U128(100), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn per_task_limit_enforced() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    contract.create_task(worker(), U128(2 * ONE_NEAR), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
#[should_panic(expected = "Daily limit exceeded")]
fn daily_limit_enforced() {
    let mut contract = setup();
    testing_env!(context(owner(), 1));
    contract.set_policy(U128(ONE_NEAR), U128(100));
    testing_env!(context(buyer(), 1));
    contract.create_task(worker(), U128(200), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
fn daily_limit_resets_at_utc_day_boundary() {
    let mut contract = setup_at(NS_PER_DAY - 1);
    testing_env!(context(owner(), NS_PER_DAY - 1));
    contract.set_policy(U128(ONE_NEAR), U128(100));

    testing_env!(context(buyer(), NS_PER_DAY - 1));
    contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), u64::MAX);
    let (_limit, spent, day) = contract.get_daily_status();
    assert_eq!(spent.0, 100);
    assert_eq!(day, 0);
    assert_invariants(&contract);

    testing_env!(context(buyer(), NS_PER_DAY));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), u64::MAX);
    let (_limit, spent, day) = contract.get_daily_status();
    assert_eq!(spent.0, 100);
    assert_eq!(day, 1);
    assert_eq!(task_id, 2);
    assert_invariants(&contract);
}

#[test]
fn pass_moves_to_payout_pending_then_settled() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );

    assert_eq!(contract.get_task_state(task_id).unwrap(), "PayoutPending");
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 10 * ONE_NEAR - 100);
    assert_eq!(reserved.0, 0);
    assert_eq!(contract.get_pending_out().0, 100);
    assert_invariants(&contract);

    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_payout(task_id);

    assert_eq!(contract.get_task_state(task_id).unwrap(), "Settled");
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR - 100);
    assert_eq!(available.0, 10 * ONE_NEAR - 100);
    assert_eq!(reserved.0, 0);
    assert_eq!(contract.get_pending_out().0, 0);
    assert_invariants(&contract);
}

#[test]
fn fail_verification_refunds() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), false);
    verify(
        &mut contract,
        task_id,
        false,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );

    assert_eq!(contract.get_task_state(task_id).unwrap(), "Refunded");
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 10 * ONE_NEAR);
    assert_eq!(reserved.0, 0);
    assert_eq!(contract.get_daily_status().1 .0, 0);
    assert_invariants(&contract);
}

#[test]
#[should_panic(expected = "Invalid verifier signature")]
fn invalid_verifier_signature_rejected() {
    let (mut contract, task_id) = submitted_task();
    let mut sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    sig[0] ^= 0xff;
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );
}

#[test]
#[should_panic(expected = "Criteria hash mismatch")]
fn incorrect_criteria_hash_rejected() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(9), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(9),
        hash_n(2),
        hash_n(3),
        sig,
    );
}

#[test]
#[should_panic(expected = "Input hash mismatch")]
fn incorrect_input_hash_rejected() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(9), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(9),
        hash_n(3),
        sig,
    );
}

#[test]
#[should_panic(expected = "Artifact hash mismatch")]
fn incorrect_artifact_hash_rejected() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(9), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(9),
        sig,
    );
}

#[test]
#[should_panic(expected = "Not in Submitted state")]
fn replayed_verification_rejected() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig.clone(),
    );
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );
}

#[test]
fn expired_task_restores_available() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 50);

    testing_env!(context(submitter(), 50));
    contract.expire_task(task_id);
    assert_eq!(contract.get_task_state(task_id).unwrap(), "Expired");
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 10 * ONE_NEAR);
    assert_eq!(reserved.0, 0);
    assert_eq!(contract.get_daily_status().1 .0, 0);
    assert_invariants(&contract);
}

#[test]
#[should_panic(expected = "Cannot expire")]
fn expired_task_cannot_be_expired_twice() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 50);
    testing_env!(context(submitter(), 50));
    contract.expire_task(task_id);
    contract.expire_task(task_id);
}

#[test]
fn failed_payout_restores_balances_once() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );

    env_with_promises(context(contract_id(), 1), vec![PromiseResult::Failed]);
    contract.on_payout(task_id);
    assert_eq!(contract.get_task_state(task_id).unwrap(), "Refunded");
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 10 * ONE_NEAR);
    assert_eq!(reserved.0, 0);
    assert_eq!(contract.get_pending_out().0, 0);
    assert_eq!(contract.get_daily_status().1 .0, 0);
    assert_invariants(&contract);

    env_with_promises(context(contract_id(), 1), vec![PromiseResult::Failed]);
    contract.on_payout(task_id);
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 10 * ONE_NEAR);
    assert_eq!(reserved.0, 0);
    assert_invariants(&contract);
}

#[test]
fn successful_withdrawal_and_failed_withdrawal_recovery() {
    let mut contract = setup();
    testing_env!(context(owner(), 1));
    contract.withdraw(U128(ONE_NEAR));
    assert_eq!(contract.get_pending_out().0, ONE_NEAR);
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(total.0, 10 * ONE_NEAR);
    assert_eq!(available.0, 9 * ONE_NEAR);
    assert_eq!(reserved.0, 0);
    assert_invariants(&contract);

    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_withdraw(1);
    let (total, available, _) = contract.get_treasury();
    assert_eq!(total.0, 9 * ONE_NEAR);
    assert_eq!(available.0, 9 * ONE_NEAR);
    assert_eq!(contract.get_pending_out().0, 0);
    assert_invariants(&contract);

    testing_env!(context(owner(), 1));
    contract.withdraw(U128(ONE_NEAR));
    env_with_promises(context(contract_id(), 1), vec![PromiseResult::Failed]);
    contract.on_withdraw(2);
    let (total, available, _) = contract.get_treasury();
    assert_eq!(total.0, 9 * ONE_NEAR);
    assert_eq!(available.0, 9 * ONE_NEAR);
    assert_eq!(contract.get_pending_out().0, 0);
    assert_invariants(&contract);

    env_with_promises(context(contract_id(), 1), vec![PromiseResult::Failed]);
    contract.on_withdraw(2);
    let (total, available, _) = contract.get_treasury();
    assert_eq!(total.0, 9 * ONE_NEAR);
    assert_eq!(available.0, 9 * ONE_NEAR);
    assert_invariants(&contract);
}

#[test]
fn reserved_amount_cannot_be_withdrawn_or_paid_twice() {
    let mut contract = setup();
    let task_id = create_funded_task(&mut contract, ONE_NEAR, u64::MAX);
    let (total, available, reserved) = contract.get_treasury();
    assert_eq!(available.0, 9 * ONE_NEAR);
    assert_eq!(reserved.0, ONE_NEAR);
    assert_eq!(total.0, 10 * ONE_NEAR);

    testing_env!(context(owner(), 1));
    contract.withdraw(U128(9 * ONE_NEAR));
    assert_eq!(contract.get_treasury().1 .0, 0);
    assert_eq!(contract.get_treasury().2 .0, ONE_NEAR);
    assert_invariants(&contract);

    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_withdraw(1);

    submit_work(&mut contract, task_id);
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );
    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_payout(task_id);
    let after = contract.get_treasury();
    assert_eq!(after.0 .0, 0);
    assert_eq!(after.2 .0, 0);
    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_payout(task_id);
    assert_eq!(contract.get_treasury(), after);
    assert_eq!(contract.get_task_state(task_id).unwrap(), "Settled");
    assert_invariants(&contract);
}

#[test]
#[should_panic(expected = "Insufficient available")]
fn cannot_withdraw_reserved_funds() {
    let mut contract = setup();
    let _ = create_funded_task(&mut contract, ONE_NEAR, u64::MAX);
    testing_env!(context(owner(), 1));
    contract.withdraw(U128(10 * ONE_NEAR));
}

#[test]
#[should_panic(expected = "Only contract")]
fn unauthorized_payout_callback_rejected() {
    let (mut contract, task_id) = submitted_task();
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );
    env_with_promises(context(outsider(), 1), vec![PromiseResult::Failed]);
    contract.on_payout(task_id);
}

#[test]
#[should_panic(expected = "Only contract")]
fn unauthorized_withdraw_callback_rejected() {
    let mut contract = setup();
    testing_env!(context(owner(), 1));
    contract.withdraw(U128(100));
    env_with_promises(context(outsider(), 1), vec![PromiseResult::Failed]);
    contract.on_withdraw(1);
}

#[test]
#[should_panic(expected = "Unauthorized depositor")]
fn unauthorized_deposit_rejected() {
    let mut contract = setup();
    testing_env!(context_deposit(outsider(), ONE_NEAR, 1));
    contract.deposit();
}

#[test]
fn successful_payout_keeps_daily_spent() {
    let (mut contract, task_id) = submitted_task();
    assert_eq!(contract.get_daily_status().1 .0, 100);
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    verify(
        &mut contract,
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        sig,
    );
    env_with_promises(
        context(contract_id(), 1),
        vec![PromiseResult::Successful(vec![])],
    );
    contract.on_payout(task_id);
    assert_eq!(contract.get_task_state(task_id).unwrap(), "Settled");
    assert_eq!(contract.get_daily_status().1 .0, 100);
}

#[test]
fn network_id_is_testnet_and_binds_signatures() {
    let contract = setup();
    assert_eq!(contract.get_network_id(), "testnet");
    assert_eq!(contract.get_protocol_id(), "pactara-v1");
    assert_eq!(contract.get_code_version(), "pactara-2.1.0");
    testing_env!(context_deposit(owner(), ONE_NEAR, 1));
    let other = Pactara::new_with_network(
        owner(),
        U128(ONE_NEAR),
        U128(5 * ONE_NEAR),
        "mainnet".to_string(),
    );
    assert_eq!(other.get_network_id(), "mainnet");
    let a = contract.get_signing_message(1, hash_n(1), hash_n(2), hash_n(3), true);
    let b = other.get_signing_message(1, hash_n(1), hash_n(2), hash_n(3), true);
    assert_ne!(a, b);
}

#[test]
#[should_panic(expected = "Bad deadline")]
fn invalid_deadline_rejected() {
    let mut contract = setup();
    testing_env!(context(buyer(), 100));
    contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 50);
}

#[test]
#[should_panic(expected = "Insufficient available")]
fn insufficient_treasury_rejected() {
    testing_env!(context_deposit(owner(), 50, 1));
    let mut contract = Pactara::new(owner(), U128(ONE_NEAR), U128(5 * ONE_NEAR));
    contract.deposit();
    contract.add_authorized_buyer(buyer());
    contract.add_authorized_worker(worker());
    testing_env!(context(buyer(), 1));
    contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
#[should_panic(expected = "Task not found")]
fn unknown_task_verification_rejected() {
    let mut contract = setup();
    testing_env!(context(submitter(), 1));
    contract.submit_verification(99, true, hash_n(1), hash_n(2), hash_n(3), vec![0u8; 64]);
}

#[test]
#[should_panic(expected = "Only worker")]
fn only_named_worker_can_submit_artifact() {
    let mut contract = setup();
    let task_id = create_funded_task(&mut contract, 100, u64::MAX);
    testing_env!(context(outsider(), 1));
    contract.submit_artifact(task_id, hash_n(3));
}

#[test]
#[should_panic(expected = "Only owner")]
fn buyer_cannot_set_policy() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    contract.set_policy(U128(1), U128(1));
}

#[test]
#[should_panic(expected = "Only owner")]
fn buyer_cannot_withdraw() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    contract.withdraw(U128(1));
}

#[test]
#[should_panic(expected = "Not expired")]
fn expire_before_deadline_rejected() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 5_000);
    testing_env!(context(submitter(), 50));
    contract.expire_task(task_id);
}

#[test]
fn get_policy_exposes_limits_for_preflight() {
    let contract = setup();
    let (max_task, daily_limit, daily_spent, _day) = contract.get_policy();
    assert_eq!(max_task.0, ONE_NEAR);
    assert_eq!(daily_limit.0, 5 * ONE_NEAR);
    assert_eq!(daily_spent.0, 0);
    assert_invariants(&contract);
}

#[test]
#[should_panic(expected = "No verifier registered")]
fn unregistered_verifier_rejected() {
    testing_env!(context_deposit(owner(), 10 * ONE_NEAR, 1));
    let mut contract = Pactara::new(owner(), U128(ONE_NEAR), U128(5 * ONE_NEAR));
    contract.deposit();
    contract.add_authorized_buyer(buyer());
    contract.add_authorized_worker(worker());
    let task_id = create_funded_task(&mut contract, 100, u64::MAX);
    submit_work(&mut contract, task_id);
    testing_env!(context(submitter(), 1));
    contract.submit_verification(
        task_id,
        true,
        hash_n(1),
        hash_n(2),
        hash_n(3),
        vec![0u8; 64],
    );
}

#[test]
#[should_panic(expected = "Deadline passed")]
fn verification_after_deadline_rejected() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 1_000);
    testing_env!(context(worker(), 2));
    contract.submit_artifact(task_id, hash_n(3));
    let sig = sign(&contract, task_id, hash_n(1), hash_n(2), hash_n(3), true);
    testing_env!(context(submitter(), 2_000));
    contract.submit_verification(task_id, true, hash_n(1), hash_n(2), hash_n(3), sig);
}

#[test]
#[should_panic(expected = "Wrong state")]
fn duplicate_artifact_submission_rejected() {
    let mut contract = setup();
    let task_id = create_funded_task(&mut contract, 100, u64::MAX);
    submit_work(&mut contract, task_id);
    submit_work(&mut contract, task_id);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn zero_amount_rejected() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    contract.create_task(worker(), U128(0), hash_n(1), hash_n(2), u64::MAX);
}

#[test]
fn expire_submitted_task_refunds() {
    let mut contract = setup();
    testing_env!(context(buyer(), 1));
    let task_id = contract.create_task(worker(), U128(100), hash_n(1), hash_n(2), 1_000);
    testing_env!(context(worker(), 2));
    contract.submit_artifact(task_id, hash_n(3));
    testing_env!(context(submitter(), 1_000));
    contract.expire_task(task_id);
    assert_eq!(contract.get_task_state(task_id).unwrap(), "Expired");
    assert_eq!(contract.get_treasury().1 .0, 10 * ONE_NEAR);
    assert_invariants(&contract);
}
