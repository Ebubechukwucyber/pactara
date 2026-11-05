use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::near;
use near_sdk::store::LookupMap;
use near_sdk::{env, AccountId, NearToken, PanicOnDefault, Promise};

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub enum TaskStatus {
    Funded,
    Submitted,
    Settled,
    Expired,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Task {
    pub id: u64,
    pub worker: AccountId,
    pub amount: u128,
    pub status: TaskStatus,
    pub criteria_hash: [u8; 32],
    pub input_hash: [u8; 32],
    pub artifact_hash: Option<[u8; 32]>,
    pub verification_deadline: u64,
    pub created_at: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Pactara {
    pub owner: AccountId,
    pub available_balance: u128,
    pub reserved_balance: u128,
    pub max_amount_per_task: u128,
    pub tasks: LookupMap<u64, Task>,
    pub next_task_id: u64,
    pub verifier_keys: Vec<String>,
}

#[near]
impl Pactara {
    #[init]
    pub fn new(owner: AccountId, max_amount_per_task: U128, _daily_limit: U128) -> Self {
        Self {
            owner,
            available_balance: 0,
            reserved_balance: 0,
            max_amount_per_task: max_amount_per_task.0,
            tasks: LookupMap::new(b"t"),
            next_task_id: 1,
            verifier_keys: Vec::new(),
        }
    }

   #[payable]
    pub fn deposit(&mut self) {
    let amount = env::attached_deposit().as_yoctonear();
    self.available_balance += amount;
    env::log_str(&format!("Deposited {}", amount));
}
    

    pub fn withdraw(&mut self, amount: U128) {
        self.assert_owner();
        let amount = amount.0;
        assert!(self.available_balance >= amount, "Insufficient available balance");
        self.available_balance -= amount;
        Promise::new(self.owner.clone()).transfer(NearToken::from_yoctonear(amount));
    }

    pub fn register_verifier(&mut self, public_key: String) {
        self.assert_owner();
        self.verifier_keys.push(public_key);
        env::log_str("Verifier registered");
    }

    pub fn create_task(
        &mut self,
        worker: AccountId,
        amount: U128,
        criteria_hash: [u8; 32],
        input_hash: [u8; 32],
        verification_deadline: u64,
    ) -> u64 {
        self.assert_owner();
        let amount = amount.0;
        assert!(amount <= self.max_amount_per_task, "Amount exceeds max");
        assert!(self.available_balance >= amount, "Insufficient treasury balance");

        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let task = Task {
            id: task_id,
            worker: worker.clone(),
            amount,
            status: TaskStatus::Funded,
            criteria_hash,
            input_hash,
            artifact_hash: None,
            verification_deadline,
            created_at: env::block_timestamp(),
        };

        self.tasks.insert(task_id, task);
        self.available_balance -= amount;
        self.reserved_balance += amount;

        env::log_str(&format!(
            "Task {} created | worker: {} | amount: {}",
            task_id, worker, amount
        ));
        task_id
    }

    pub fn submit_artifact(&mut self, task_id: u64, artifact_hash: [u8; 32]) {
        let mut task = self.tasks.get(&task_id).expect("Task not found").clone();
        assert!(
            env::predecessor_account_id() == task.worker,
            "Only worker can submit artifact"
        );
        assert!(
            matches!(task.status, TaskStatus::Funded),
            "Task not in Funded state"
        );

        task.artifact_hash = Some(artifact_hash);
        task.status = TaskStatus::Submitted;
        self.tasks.insert(task_id, task);

        env::log_str(&format!("Artifact submitted for task {}", task_id));
    }

    /// Temporary test-only method. Owner can force-settle a Submitted task.
    /// REMOVE BEFORE PRODUCTION.
    pub fn test_settle(&mut self, task_id: u64) {
        self.assert_owner();
        let mut task = self.tasks.get(&task_id).expect("Task not found").clone();
        assert!(
            matches!(task.status, TaskStatus::Submitted),
            "Task not submitted"
        );

        Promise::new(task.worker.clone())
            .transfer(NearToken::from_yoctonear(task.amount));

        self.reserved_balance -= task.amount;
        task.status = TaskStatus::Settled;
        self.tasks.insert(task_id, task);

        env::log_str(&format!("Task {} force-settled (test)", task_id));
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_treasury(&self) -> (U128, U128, U128) {
        (
            U128(self.available_balance),
            U128(self.reserved_balance),
            U128(self.max_amount_per_task),
        )
    }

    pub fn get_task_count(&self) -> u64 {
        self.next_task_id
    }

    pub fn get_verifier_keys(&self) -> Vec<String> {
        self.verifier_keys.clone()
    }

    pub fn get_task(&self, task_id: u64) -> String {
        match self.tasks.get(&task_id) {
            Some(task) => format!(
                "id={} worker={} amount={} status={:?} criteria={:?} input={:?} artifact={:?} deadline={} created={}",
                task.id,
                task.worker,
                task.amount,
                task.status,
                task.criteria_hash,
                task.input_hash,
                task.artifact_hash,
                task.verification_deadline,
                task.created_at
            ),
            None => "Task not found".to_string(),
        }
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can call this"
        );
    }
}