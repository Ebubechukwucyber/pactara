//! Pactara: reserve funds, bind hashes, settle on an authorized verifier signature.
//!
//! Architectural limits that this contract does **not** lift:
//! - One owner-registered ed25519 verifier; no TEE, quorum, or objective work check.
//! - Documents live off-chain; only SHA-256 hashes are stored.
//! - A signed result is a bearer credential until the task nonce is consumed.
//! - Failed payout/verification closes the task; there is no retry on the same id.
//! - `PayoutPending` cannot be expired. Do not re-issue the transfer (double-pay risk).
//!   Callbacks are written not to panic after the private-predecessor check.
//! - Maps are not iterable. Withdrawals always pay the owner.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::near;
use near_sdk::serde::Serialize;
use near_sdk::store::LookupMap;
use near_sdk::{
    env, log, AccountId, CryptoHash, Gas, NearToken, PanicOnDefault, Promise, PromiseResult,
    PublicKey,
};

const PROTOCOL_ID: &[u8] = b"pactara-v1";
const CODE_VERSION: &str = "pactara-2.0.0";
const DEFAULT_NETWORK_ID: &str = "testnet";
const CALLBACK_GAS: Gas = Gas::from_tgas(50);

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq)]
pub enum TaskState {
    Funded,
    Submitted,
    Settled,
    Refunded,
    Expired,
    PayoutPending,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Task {
    pub id: u64,
    pub buyer: AccountId,
    pub worker: AccountId,
    pub amount: u128,
    pub criteria_hash: CryptoHash,
    pub input_hash: CryptoHash,
    pub artifact_hash: Option<CryptoHash>,
    pub state: TaskState,
    pub verification_deadline: u64,
    pub nonce_used: bool,
    pub created_day: u64,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TaskView {
    pub id: u64,
    pub buyer: AccountId,
    pub worker: AccountId,
    pub amount: U128,
    pub criteria_hash: CryptoHash,
    pub input_hash: CryptoHash,
    pub artifact_hash: Option<CryptoHash>,
    pub state: String,
    pub verification_deadline: u64,
    pub nonce_used: bool,
    pub created_day: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Pactara {
    pub owner: AccountId,
    pub total: u128,
    pub available: u128,
    pub reserved: u128,
    pub max_amount_per_task: u128,
    pub daily_limit: u128,
    pub daily_spent: u128,
    pub current_day: u64,
    pub tasks: LookupMap<u64, Task>,
    pub next_task_id: u64,
    pub authorized_buyers: LookupMap<AccountId, bool>,
    pub authorized_workers: LookupMap<AccountId, bool>,
    pub verifier_public_key: Option<PublicKey>,
    pub pending_payouts: LookupMap<u64, (AccountId, u128)>,
    pub pending_withdrawals: LookupMap<u64, u128>,
    pub next_withdrawal_id: u64,
    pub pending_out: u128,
    pub network_id: String,
}

#[near]
impl Pactara {
    #[init]
    pub fn new(owner: AccountId, max_amount_per_task: U128, daily_limit: U128) -> Self {
        Self::construct(owner, max_amount_per_task, daily_limit, DEFAULT_NETWORK_ID.to_string())
    }

    #[init]
    pub fn new_with_network(
        owner: AccountId,
        max_amount_per_task: U128,
        daily_limit: U128,
        network_id: String,
    ) -> Self {
        assert!(!network_id.is_empty(), "network_id required");
        Self::construct(owner, max_amount_per_task, daily_limit, network_id)
    }

    fn construct(
        owner: AccountId,
        max_amount_per_task: U128,
        daily_limit: U128,
        network_id: String,
    ) -> Self {
        let mut contract = Self {
            owner: owner.clone(),
            total: 0,
            available: 0,
            reserved: 0,
            max_amount_per_task: max_amount_per_task.0,
            daily_limit: daily_limit.0,
            daily_spent: 0,
            current_day: env::block_timestamp() / Self::nanoseconds_per_day(),
            tasks: LookupMap::new(b"t"),
            next_task_id: 1,
            authorized_buyers: LookupMap::new(b"b"),
            authorized_workers: LookupMap::new(b"w"),
            verifier_public_key: None,
            pending_payouts: LookupMap::new(b"p"),
            pending_withdrawals: LookupMap::new(b"d"),
            next_withdrawal_id: 1,
            pending_out: 0,
            network_id,
        };
        contract.authorized_buyers.insert(owner.clone(), true);
        contract
    }

    fn nanoseconds_per_day() -> u64 {
        86_400_000_000_000
    }

    fn update_day(&mut self) {
        let day = env::block_timestamp() / Self::nanoseconds_per_day();
        if day != self.current_day {
            self.current_day = day;
            self.daily_spent = 0;
        }
    }

    fn restore_daily_spend(&mut self, task: &Task) {
        self.update_day();
        if self.current_day == task.created_day {
            match self.daily_spent.checked_sub(task.amount) {
                Some(remaining) => self.daily_spent = remaining,
                None => {
                    log!("daily_spent underflow; clamping to 0");
                    self.daily_spent = 0;
                }
            }
        }
    }

    fn take_pending_out(&mut self, amount: u128) {
        match self.pending_out.checked_sub(amount) {
            Some(remaining) => self.pending_out = remaining,
            None => {
                log!(
                    "pending_out underflow (had {}, removing {}); clamping to 0",
                    self.pending_out,
                    amount
                );
                self.pending_out = 0;
            }
        }
    }

    fn return_reserved_to_available(&mut self, task: &Task) {
        self.reserved -= task.amount;
        self.available += task.amount;
        self.restore_daily_spend(task);
    }

    fn assert_accounting(&self) {
        let summed = self
            .available
            .checked_add(self.reserved)
            .and_then(|v| v.checked_add(self.pending_out));
        assert_eq!(Some(self.total), summed, "treasury accounting invariant");
        assert!(
            self.total >= self.available + self.reserved,
            "total >= available + reserved"
        );
    }

    fn log_if_accounting_broken(&self) {
        let summed = self
            .available
            .checked_add(self.reserved)
            .and_then(|v| v.checked_add(self.pending_out));
        if summed != Some(self.total) {
            log!(
                "accounting warning total={} available={} reserved={} pending_out={}",
                self.total,
                self.available,
                self.reserved,
                self.pending_out
            );
        }
    }

    fn previous_transfer_succeeded() -> bool {
        env::promise_results_count() == 1
            && matches!(env::promise_result(0), PromiseResult::Successful(_))
    }

    fn can_deposit(&self, account: &AccountId) -> bool {
        self.authorized_buyers.get(account).copied().unwrap_or(false)
    }

    fn build_message(
        &self,
        task_id: u64,
        criteria_hash: &CryptoHash,
        input_hash: &CryptoHash,
        artifact_hash: &CryptoHash,
        passed: bool,
    ) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(PROTOCOL_ID);
        message.extend_from_slice(self.network_id.as_bytes());
        message.extend_from_slice(env::current_account_id().as_bytes());
        message.extend_from_slice(&task_id.to_le_bytes());
        message.extend_from_slice(criteria_hash.as_ref());
        message.extend_from_slice(input_hash.as_ref());
        message.extend_from_slice(artifact_hash.as_ref());
        message.push(if passed { 1 } else { 0 });
        message
    }

    fn ed25519_raw_key(verifier_key: &PublicKey) -> [u8; 32] {
        let key_bytes = verifier_key.as_bytes();
        assert!(
            key_bytes.len() == 33 && key_bytes[0] == 0,
            "Verifier public key must be ed25519"
        );
        key_bytes[1..].try_into().expect("ed25519 raw key")
    }

    fn task_view(task: &Task) -> TaskView {
        TaskView {
            id: task.id,
            buyer: task.buyer.clone(),
            worker: task.worker.clone(),
            amount: U128(task.amount),
            criteria_hash: task.criteria_hash,
            input_hash: task.input_hash,
            artifact_hash: task.artifact_hash,
            state: format!("{:?}", task.state),
            verification_deadline: task.verification_deadline,
            nonce_used: task.nonce_used,
            created_day: task.created_day,
        }
    }

    #[payable]
    pub fn deposit(&mut self) {
        let caller = env::predecessor_account_id();
        assert!(self.can_deposit(&caller), "Unauthorized depositor");
        let amount = env::attached_deposit().as_yoctonear();
        self.total += amount;
        self.available += amount;
        self.assert_accounting();
        log!("Deposit: {}", amount);
    }

    pub fn withdraw(&mut self, amount: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        let amount = amount.0;
        assert!(amount > 0, "Amount must be positive");
        assert!(amount <= self.available, "Insufficient available");

        self.available -= amount;
        self.pending_out += amount;

        let withdrawal_id = self.next_withdrawal_id;
        self.next_withdrawal_id += 1;
        self.pending_withdrawals.insert(withdrawal_id, amount);
        self.assert_accounting();

        let _ = Promise::new(self.owner.clone())
            .transfer(NearToken::from_yoctonear(amount))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_withdraw(withdrawal_id),
            );
        log!("Withdrawal {} pending: {}", withdrawal_id, amount);
    }

    #[private]
    pub fn on_withdraw(&mut self, withdrawal_id: u64) {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Only contract"
        );
        let success = Self::previous_transfer_succeeded();

        if let Some(amount) = self.pending_withdrawals.remove(&withdrawal_id) {
            self.take_pending_out(amount);
            if success {
                match self.total.checked_sub(amount) {
                    Some(remaining) => self.total = remaining,
                    None => {
                        log!("total underflow on withdraw success; clamping to 0");
                        self.total = 0;
                    }
                }
                log!("Withdrawal {} succeeded", withdrawal_id);
            } else {
                self.available += amount;
                log!("Withdrawal {} failed, restored {}", withdrawal_id, amount);
            }
            self.log_if_accounting_broken();
        }
    }

    pub fn set_policy(&mut self, max_amount_per_task: U128, daily_limit: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.max_amount_per_task = max_amount_per_task.0;
        self.daily_limit = daily_limit.0;
        log!("Policy updated");
    }

    pub fn add_authorized_buyer(&mut self, buyer: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_buyers.insert(buyer.clone(), true);
        log!("Authorized buyer: {}", buyer);
    }

    pub fn remove_authorized_buyer(&mut self, buyer: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        assert_ne!(buyer, self.owner, "Cannot remove owner as buyer");
        self.authorized_buyers.insert(buyer.clone(), false);
        log!("Removed buyer: {}", buyer);
    }

    pub fn add_authorized_worker(&mut self, worker: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_workers.insert(worker.clone(), true);
        log!("Authorized worker: {}", worker);
    }

    pub fn remove_authorized_worker(&mut self, worker: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_workers.insert(worker.clone(), false);
        log!("Removed worker: {}", worker);
    }

    pub fn register_verifier(&mut self, public_key: PublicKey) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        let _ = Self::ed25519_raw_key(&public_key);
        self.verifier_public_key = Some(public_key);
        log!("Verifier registered");
    }

    pub fn create_task(
        &mut self,
        worker: AccountId,
        amount: U128,
        criteria_hash: CryptoHash,
        input_hash: CryptoHash,
        verification_deadline: u64,
    ) -> u64 {
        let caller = env::predecessor_account_id();
        assert!(
            self.authorized_buyers.get(&caller).copied().unwrap_or(false),
            "Unauthorized buyer"
        );
        assert!(
            self.authorized_workers.get(&worker).copied().unwrap_or(false),
            "Unauthorized worker"
        );

        self.update_day();

        let amount = amount.0;
        assert!(amount > 0 && amount <= self.max_amount_per_task, "Invalid amount");
        assert!(amount <= self.available, "Insufficient available");
        assert!(
            self.daily_spent + amount <= self.daily_limit,
            "Daily limit exceeded"
        );
        assert!(verification_deadline > env::block_timestamp(), "Bad deadline");

        self.available -= amount;
        self.reserved += amount;
        self.daily_spent += amount;

        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let task = Task {
            id: task_id,
            buyer: caller,
            worker: worker.clone(),
            amount,
            criteria_hash,
            input_hash,
            artifact_hash: None,
            state: TaskState::Funded,
            verification_deadline,
            nonce_used: false,
            created_day: self.current_day,
        };
        self.tasks.insert(task_id, task);
        self.assert_accounting();
        log!("Task {} created | worker: {} | amount: {}", task_id, worker, amount);
        task_id
    }

    pub fn submit_artifact(&mut self, task_id: u64, artifact_hash: CryptoHash) {
        let mut task = self.tasks.get(&task_id).expect("Task not found").clone();
        assert_eq!(env::predecessor_account_id(), task.worker, "Only worker");
        assert_eq!(task.state, TaskState::Funded, "Wrong state");
        assert!(
            env::block_timestamp() < task.verification_deadline,
            "Deadline passed"
        );

        task.artifact_hash = Some(artifact_hash);
        task.state = TaskState::Submitted;
        self.tasks.insert(task_id, task);
        log!("Artifact submitted for task {}", task_id);
    }

    pub fn submit_verification(
        &mut self,
        task_id: u64,
        passed: bool,
        criteria_hash: CryptoHash,
        input_hash: CryptoHash,
        artifact_hash: CryptoHash,
        signature: Vec<u8>,
    ) {
        let mut task = self.tasks.get(&task_id).expect("Task not found").clone();
        assert_eq!(task.state, TaskState::Submitted, "Not in Submitted state");
        assert!(!task.nonce_used, "Nonce already used");
        assert!(
            env::block_timestamp() < task.verification_deadline,
            "Deadline passed"
        );

        assert_eq!(task.criteria_hash, criteria_hash, "Criteria hash mismatch");
        assert_eq!(task.input_hash, input_hash, "Input hash mismatch");
        assert_eq!(
            task.artifact_hash.expect("Artifact missing"),
            artifact_hash,
            "Artifact hash mismatch"
        );

        let verifier_key = self
            .verifier_public_key
            .as_ref()
            .expect("No verifier registered");

        let message = self.build_message(task_id, &criteria_hash, &input_hash, &artifact_hash, passed);

        assert_eq!(signature.len(), 64, "Signature must be 64 bytes");
        let sig: &[u8; 64] = signature.as_slice().try_into().expect("signature length");
        let pk = Self::ed25519_raw_key(verifier_key);

        assert!(
            env::ed25519_verify(sig, &message, &pk),
            "Invalid verifier signature"
        );

        task.nonce_used = true;

        if passed {
            task.state = TaskState::PayoutPending;
            self.reserved -= task.amount;
            self.pending_out += task.amount;
            self.pending_payouts
                .insert(task_id, (task.worker.clone(), task.amount));
            self.tasks.insert(task_id, task.clone());
            self.assert_accounting();

            let _ = Promise::new(task.worker.clone())
                .transfer(NearToken::from_yoctonear(task.amount))
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(CALLBACK_GAS)
                        .on_payout(task_id),
                );
            log!("Payout pending for task {} | paying {}", task_id, task.worker);
        } else {
            self.return_reserved_to_available(&task);
            task.state = TaskState::Refunded;
            self.tasks.insert(task_id, task);
            self.assert_accounting();
            log!("REFUNDED task {}", task_id);
        }
    }

    #[private]
    pub fn on_payout(&mut self, task_id: u64) {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Only contract"
        );
        let success = Self::previous_transfer_succeeded();

        if let Some((_worker, amount)) = self.pending_payouts.remove(&task_id) {
            self.take_pending_out(amount);
            if let Some(mut task) = self.tasks.get(&task_id).cloned() {
                if task.state == TaskState::PayoutPending {
                    if success {
                        match self.total.checked_sub(amount) {
                            Some(remaining) => self.total = remaining,
                            None => {
                                log!("total underflow on payout success; clamping to 0");
                                self.total = 0;
                            }
                        }
                        task.state = TaskState::Settled;
                        log!("Payout succeeded for task {}", task_id);
                    } else {
                        self.available += amount;
                        self.restore_daily_spend(&task);
                        task.state = TaskState::Refunded;
                        log!("Payout failed for task {}, refunded", task_id);
                    }
                    self.tasks.insert(task_id, task);
                } else if success {
                    match self.total.checked_sub(amount) {
                        Some(remaining) => self.total = remaining,
                        None => self.total = 0,
                    }
                } else {
                    self.available += amount;
                }
            } else if success {
                match self.total.checked_sub(amount) {
                    Some(remaining) => self.total = remaining,
                    None => self.total = 0,
                }
            } else {
                self.available += amount;
            }
            self.log_if_accounting_broken();
        }
    }

    pub fn expire_task(&mut self, task_id: u64) {
        let mut task = self.tasks.get(&task_id).expect("Task not found").clone();
        assert!(
            matches!(task.state, TaskState::Funded | TaskState::Submitted),
            "Cannot expire"
        );
        assert!(
            env::block_timestamp() >= task.verification_deadline,
            "Not expired"
        );

        self.return_reserved_to_available(&task);
        task.state = TaskState::Expired;
        self.tasks.insert(task_id, task);
        self.assert_accounting();
        log!("EXPIRED task {}", task_id);
    }

    pub fn get_treasury(&self) -> (U128, U128, U128) {
        (U128(self.total), U128(self.available), U128(self.reserved))
    }

    pub fn get_pending_out(&self) -> U128 {
        U128(self.pending_out)
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_network_id(&self) -> String {
        self.network_id.clone()
    }

    pub fn get_protocol_id(&self) -> String {
        String::from_utf8_lossy(PROTOCOL_ID).into_owned()
    }

    pub fn get_code_version(&self) -> String {
        CODE_VERSION.to_string()
    }

    pub fn get_verifier_public_key(&self) -> Option<PublicKey> {
        self.verifier_public_key.clone()
    }

    pub fn is_authorized_buyer(&self, account: AccountId) -> bool {
        self.authorized_buyers.get(&account).copied().unwrap_or(false)
    }

    pub fn is_authorized_worker(&self, account: AccountId) -> bool {
        self.authorized_workers.get(&account).copied().unwrap_or(false)
    }

    pub fn get_daily_status(&self) -> (U128, U128, u64) {
        (U128(self.daily_limit), U128(self.daily_spent), self.current_day)
    }

    pub fn get_policy(&self) -> (U128, U128, U128, u64) {
        (
            U128(self.max_amount_per_task),
            U128(self.daily_limit),
            U128(self.daily_spent),
            self.current_day,
        )
    }

    pub fn get_task_state(&self, task_id: u64) -> Option<String> {
        self.tasks.get(&task_id).map(|t| format!("{:?}", t.state))
    }

    pub fn get_task(&self, task_id: u64) -> Option<TaskView> {
        self.tasks.get(&task_id).map(Self::task_view)
    }

    pub fn get_signing_message(
        &self,
        task_id: u64,
        criteria_hash: CryptoHash,
        input_hash: CryptoHash,
        artifact_hash: CryptoHash,
        passed: bool,
    ) -> Vec<u8> {
        self.build_message(task_id, &criteria_hash, &input_hash, &artifact_hash, passed)
    }
}

#[cfg(test)]
mod tests;
