#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, AccountId, CryptoHash};

    fn setup() -> Pactara {
        let context = VMContextBuilder::new()
            .predecessor_account_id(accounts(0))
            .attached_deposit(NearToken::from_near(10))
            .build();
        testing_env!(context);
        let mut contract = Pactara::new(
            accounts(0),
            U128(1_000_000_000_000_000_000_000_000),
            U128(5_000_000_000_000_000_000_000_000),
        );
        contract.deposit();
        contract.add_authorized_buyer(accounts(1));
        contract.add_authorized_worker(accounts(2));
        contract
    }

    #[test]
    fn unauthorized_buyer_rejected() {
        let mut contract = setup();
        let context = VMContextBuilder::new()
            .predecessor_account_id(accounts(9))
            .build();
        testing_env!(context);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.create_task(
                accounts(2),
                U128(100),
                CryptoHash::default(),
                CryptoHash::default(),
                u64::MAX,
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn unauthorized_worker_rejected() {
        let mut contract = setup();
        let context = VMContextBuilder::new()
            .predecessor_account_id(accounts(1))
            .build();
        testing_env!(context);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.create_task(
                accounts(9),
                U128(100),
                CryptoHash::default(),
                CryptoHash::default(),
                u64::MAX,
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn per_task_limit_enforced() {
        let mut contract = setup();
        let context = VMContextBuilder::new()
            .predecessor_account_id(accounts(1))
            .build();
        testing_env!(context);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.create_task(
                accounts(2),
                U128(2_000_000_000_000_000_000_000_000),
                CryptoHash::default(),
                CryptoHash::default(),
                u64::MAX,
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn daily_limit_enforced() {
        let mut contract = setup();
        contract.set_policy(U128(1_000_000_000_000_000_000_000_000), U128(100));
        let context = VMContextBuilder::new()
            .predecessor_account_id(accounts(1))
            .build();
        testing_env!(context);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.create_task(
                accounts(2),
                U128(200),
                CryptoHash::default(),
                CryptoHash::default(),
                u64::MAX,
            );
        }));
        assert!(result.is_err());
    }
}
