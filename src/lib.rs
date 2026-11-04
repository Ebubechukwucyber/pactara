use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{env, log, near, AccountId, CryptoHash, NearToken, PanicOnDefault, Promise, PublicKey};

const PROTOCOL_ID: &[u8] = b"PACTARA-v1";

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Clone, Debug)]
pub enum TaskState {
    Funded,
    Submitted,
    Settled,
    Refunded,
    Expired,
}

#[derive(BorshDeserialize, BorshSerialize)]
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
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Pactara {
    total: u128,
    available: u128,
    reserved: u128,
    owner: AccountId,
    max_amount_per_task: u128,
    tasks: LookupMap<u64, Task>,
    next_task_id: u64,
    verifier_public_key: Option<PublicKey>,
    used_nonces: LookupMap<u64, bool>,
    authorized_buyers: LookupMap<AccountId, bool>,
    authorized_workers: LookupMap<AccountId, bool>,
    daily_limit: u128,
    daily_spent: u128,
    current_day: u64,
    pending_payouts: LookupMap<u64, (AccountId, u128)>,
}

#[near]
impl Pactara {
    #[init]
    pub fn new(owner: AccountId, max_amount_per_task: U128, daily_limit: U128) -> Self {
        let mut contract = Self {
            total: 0,
            available: 0,
            reserved: 0,
            owner: owner.clone(),
            max_amount_per_task: max_amount_per_task.0,
            tasks: LookupMap::new(b"t"),
            next_task_id: 1,
            verifier_public_key: None,
            used_nonces: LookupMap::new(b"n"),
            authorized_buyers: LookupMap::new(b"b"),
            authorized_workers: LookupMap::new(b"w"),
            daily_limit: daily_limit.0,
            daily_spent: 0,
            current_day: env::block_timestamp() / 86_400_000_000_000,
            pending_payouts: LookupMap::new(b"p"),
        };
        contract.authorized_buyers.insert(&owner, &true);
        contract
    }

    fn update_day(&mut self) {
        let day = env::block_timestamp() / 86_400_000_000_000;
        if day != self.current_day {
            self.current_day = day;
            self.daily_spent = 0;
        }
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
        message.extend_from_slice(env::current_account_id().as_bytes());
        message.extend_from_slice(&task_id.to_le_bytes());
        message.extend_from_slice(criteria_hash.as_ref());
        message.extend_from_slice(input_hash.as_ref());
        message.extend_from_slice(artifact_hash.as_ref());
        message.push(if passed { 1 } else { 0 });
        message
    }

    #[payable]
    pub fn deposit(&mut self) {
        let amount = env::attached_deposit().as_yoctonear();
        self.total += amount;
        self.available += amount;
        log!("Deposit: {}", amount);
    }

    pub fn withdraw(&mut self, amount: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        let amount = amount.0;
        assert!(amount <= self.available, "Insufficient available");
        self.available -= amount;
        self.total -= amount;
        let _ = Promise::new(self.owner.clone()).transfer(NearToken::from_yoctonear(amount));
    }

    pub fn set_policy(&mut self, max_amount_per_task: U128, daily_limit: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.max_amount_per_task = max_amount_per_task.0;
        self.daily_limit = daily_limit.0;
        log!("Policy updated");
    }

    pub fn add_authorized_buyer(&mut self, buyer: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_buyers.insert(&buyer, &true);
        log!("Authorized buyer: {}", buyer);
    }

    pub fn remove_authorized_buyer(&mut self, buyer: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_buyers.insert(&buyer, &false);
        log!("Removed buyer: {}", buyer);
    }

    pub fn add_authorized_worker(&mut self, worker: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_workers.insert(&worker, &true);
        log!("Authorized worker: {}", worker);
    }

    pub fn remove_authorized_worker(&mut self, worker: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.authorized_workers.insert(&worker, &false);
        log!("Removed worker: {}", worker);
    }

    pub fn register_verifier(&mut self, public_key: PublicKey) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
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
            self.authorized_buyers.get(&caller).unwrap_or(false),
            "Unauthorized buyer"
        );
        assert!(
            self.authorized_workers.get(&worker).unwrap_or(false),
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
        };
        self.tasks.insert(&task_id, &task);
        log!(
            "Task {} created | worker: {} | amount: {}",
            task_id,
            worker,
            amount
        );
        task_id
    }

    pub fn submit_artifact(&mut self, task_id: u64, artifact_hash: CryptoHash) {
        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert_eq!(env::predecessor_account_id(), task.worker, "Only worker");
        assert_eq!(task.state, TaskState::Funded, "Wrong state");
        assert!(
            env::block_timestamp() < task.verification_deadline,
            "Deadline passed"
        );

        task.artifact_hash = Some(artifact_hash);
        task.state = TaskState::Submitted;
        self.tasks.insert(&task_id, &task);
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
        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert_eq!(task.state, TaskState::Submitted, "Not in Submitted state");
        assert!(!task.nonce_used, "Nonce already used");
        assert!(
            env::block_timestamp() < task.verification_deadline,
            "Deadline passed"
        );

        assert_eq!(task.criteria_hash, criteria_hash, "Criteria hash mismatch");
        assert_eq!(task.input_hash, input_hash, "Input hash mismatch");
        assert_eq!(
            task.artifact_hash.unwrap(),
            artifact_hash,
            "Artifact hash mismatch"
        );

        let verifier_key = self
            .verifier_public_key
            .as_ref()
            .expect("No verifier registered");

        let message = self.build_message(task_id, &criteria_hash, &input_hash, &artifact_hash, passed);

        assert!(signature.len() == 64, "Signature must be 64 bytes");
        let sig: &[u8; 64] = signature.as_slice().try_into().unwrap();
        let key_bytes = verifier_key.as_bytes();
        let pk: &[u8; 32] = key_bytes.try_into().unwrap();

        assert!(
            env::ed25519_verify(sig, &message, pk),
            "Invalid verifier signature"
        );

        task.nonce_used = true;
        self.used_nonces.insert(&task_id, &true);

        if passed {
            task.state = TaskState::Settled;
            self.reserved -= task.amount;
            self.total -= task.amount;
            self.pending_payouts.insert(&task_id, &(task.worker.clone(), task.amount));
            Promise::new(task.worker.clone())
                .transfer(NearToken::from_yoctonear(task.amount))
                .then(
                    Self::ext(env::current_account_id())
                        .on_payout_complete(task_id, true),
                );
            log!("SETTLING task {} | paying {}", task_id, task.worker);
        } else {
            task.state = TaskState::Refunded;
            self.reserved -= task.amount;
            self.available += task.amount;
            log!("REFUNDED task {}", task_id);
        }
        self.tasks.insert(&task_id, &task);
    }

    #[private]
    pub fn on_payout_complete(&mut self, task_id: u64, success: bool) {
        if !success {
            if let Some((worker, amount)) = self.pending_payouts.remove(&task_id) {
                self.total += amount;
                self.available += amount;
                if let Some(mut task) = self.tasks.get(&task_id) {
                    if task.state == TaskState::Settled {
                        task.state = TaskState::Refunded;
                        self.tasks.insert(&task_id, &task);
                    }
                }
                log!("Payout failed for task {}, refunded", task_id);
            }
        } else {
            self.pending_payouts.remove(&task_id);
        }
    }

    pub fn expire_task(&mut self, task_id: u64) {
        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert!(
            matches!(task.state, TaskState::Funded | TaskState::Submitted),
            "Cannot expire"
        );
        assert!(
            env::block_timestamp() >= task.verification_deadline,
            "Not expired"
        );

        self.reserved -= task.amount;
        self.available += task.amount;
        task.state = TaskState::Expired;
        self.tasks.insert(&task_id, &task);
        log!("EXPIRED task {}", task_id);
    }

    pub fn get_treasury(&self) -> (U128, U128, U128) {
        (U128(self.total), U128(self.available), U128(self.reserved))
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn is_authorized_buyer(&self, account: AccountId) -> bool {
        self.authorized_buyers.get(&account).unwrap_or(false)
    }

    pub fn is_authorized_worker(&self, account: AccountId) -> bool {
        self.authorized_workers.get(&account).unwrap_or(false)
    }

    pub fn get_daily_status(&self) -> (U128, U128, u64) {
        (U128(self.daily_limit), U128(self.daily_spent), self.current_day)
    }

    pub fn get_task_state(&self, task_id: u64) -> Option<String> {
        self.tasks.get(&task_id).map(|t| format!("{:?}", t.state))
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
