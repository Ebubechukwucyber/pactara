use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{env, log, near, AccountId, CryptoHash, NearToken, PanicOnDefault, Promise, PublicKey};

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
}

#[near]
impl Pactara {
    #[init]
    pub fn new(owner: AccountId, max_amount_per_task: U128) -> Self {
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
        };
        // Owner is authorized by default
        contract.authorized_buyers.insert(&owner, &true);
        contract
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

    pub fn set_policy(&mut self, max_amount_per_task: U128) {
        assert_eq!(env::predecessor_account_id(), self.owner, "Only owner");
        self.max_amount_per_task = max_amount_per_task.0;
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

        let amount = amount.0;
        assert!(amount > 0 && amount <= self.max_amount_per_task, "Invalid amount");
        assert!(amount <= self.available, "Insufficient available");
        assert!(verification_deadline > env::block_timestamp(), "Bad deadline");

        self.available -= amount;
        self.reserved += amount;

        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let task = Task {
            id: task_id,
            buyer: caller,
            worker,
            amount,
            criteria_hash,
            input_hash,
            artifact_hash: None,
            state: TaskState::Funded,
            verification_deadline,
            nonce_used: false,
        };
        self.tasks.insert(&task_id, &task);
        log!("Task {} created | worker: {} | amount: {}", task_id, task.worker, amount);
        task_id
    }

    pub fn submit_artifact(&mut self, task_id: u64, artifact_hash: CryptoHash) {
        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert_eq!(env::predecessor_account_id(), task.worker, "Only worker");
        assert_eq!(task.state, TaskState::Funded, "Wrong state");
        assert!(env::block_timestamp() < task.verification_deadline, "Deadline passed");

        task.artifact_hash = Some(artifact_hash);
        task.state = TaskState::Submitted;
        self.tasks.insert(&task_id, &task);
        log!("Artifact submitted for task {}", task_id);
    }

    pub fn submit_verification(&mut self, task_id: u64, passed: bool, _signature: Option<Vec<u8>>) {
        let caller = env::predecessor_account_id();
        assert_eq!(caller, self.owner, "Only owner for now");

        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert_eq!(task.state, TaskState::Submitted, "Not in Submitted state");
        assert!(!task.nonce_used, "Nonce already used");
        assert!(env::block_timestamp() < task.verification_deadline, "Deadline passed");

        task.nonce_used = true;
        self.used_nonces.insert(&task_id, &true);

        if passed {
            task.state = TaskState::Settled;
            self.reserved -= task.amount;
            self.total -= task.amount;
            let _ = Promise::new(task.worker.clone()).transfer(NearToken::from_yoctonear(task.amount));
            log!("SETTLED task {} | paid {}", task_id, task.worker);
        } else {
            task.state = TaskState::Refunded;
            self.reserved -= task.amount;
            self.available += task.amount;
            log!("REFUNDED task {}", task_id);
        }
        self.tasks.insert(&task_id, &task);
    }

    pub fn expire_task(&mut self, task_id: u64) {
        let mut task = self.tasks.get(&task_id).expect("Task not found");
        assert!(
            matches!(task.state, TaskState::Funded | TaskState::Submitted),
            "Cannot expire"
        );
        assert!(env::block_timestamp() >= task.verification_deadline, "Not expired");

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

    pub fn get_task_state(&self, task_id: u64) -> Option<String> {
        self.tasks.get(&task_id).map(|t| format!("{:?}", t.state))
    }
}
