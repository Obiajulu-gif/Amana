/// Issue #388 — Gas and footprint regression checks for hot paths
///
/// Measures CPU instructions and memory bytes consumed by the four hot paths:
///   create_trade, deposit, initiate_dispute, resolve_dispute
///
/// Baseline thresholds are versioned here. A regression (cost exceeding the
/// threshold) causes the test to fail, surfacing the issue in CI before it ships.
///
/// Threshold methodology
/// ---------------------
/// Values were established by running the suite once and rounding up to the
/// nearest comfortable headroom (~20 % above the measured baseline).  They are
/// intentionally conservative so that genuine regressions are caught while
/// minor SDK-version fluctuations do not produce false positives.
///
/// To re-baseline after an intentional change, update the constants below and
/// commit the diff as part of the PR that introduces the change.
#[cfg(test)]
mod gas_footprint_tests {
    use crate::{EscrowContract, EscrowContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        token, Address, Env, String,
    };

    // -----------------------------------------------------------------------
    // Versioned baseline thresholds  (v0.1 — amana_escrow 0.1.0)
    // -----------------------------------------------------------------------

    /// Maximum CPU instructions allowed for create_trade.
    const BASELINE_CREATE_TRADE_CPU: u64 = 3_000_000;
    /// Maximum memory bytes allowed for create_trade.
    const BASELINE_CREATE_TRADE_MEM: u64 = 2_000_000;

    /// Maximum CPU instructions allowed for deposit.
    const BASELINE_DEPOSIT_CPU: u64 = 5_000_000;
    /// Maximum memory bytes allowed for deposit.
    const BASELINE_DEPOSIT_MEM: u64 = 3_000_000;

    /// Maximum CPU instructions allowed for initiate_dispute.
    const BASELINE_DISPUTE_CPU: u64 = 3_000_000;
    /// Maximum memory bytes allowed for initiate_dispute.
    const BASELINE_DISPUTE_MEM: u64 = 2_000_000;

    /// Maximum CPU instructions allowed for resolve_dispute.
    const BASELINE_RESOLVE_CPU: u64 = 8_000_000;
    /// Maximum memory bytes allowed for resolve_dispute.
    const BASELINE_RESOLVE_MEM: u64 = 4_000_000;

    fn combined_lifecycle_baseline() -> (u64, u64) {
        let cpu = BASELINE_CREATE_TRADE_CPU
            .checked_add(BASELINE_DEPOSIT_CPU)
            .and_then(|v| v.checked_add(BASELINE_DISPUTE_CPU))
            .and_then(|v| v.checked_add(BASELINE_RESOLVE_CPU))
            .expect("combined CPU baseline overflowed");
        let mem = BASELINE_CREATE_TRADE_MEM
            .checked_add(BASELINE_DEPOSIT_MEM)
            .and_then(|v| v.checked_add(BASELINE_DISPUTE_MEM))
            .and_then(|v| v.checked_add(BASELINE_RESOLVE_MEM))
            .expect("combined memory baseline overflowed");

        (cpu, mem)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    struct Ctx {
        env: Env,
        contract_id: Address,
        buyer: Address,
        seller: Address,
        mediator: Address,
    }

    impl Ctx {
        fn new(amount: i128) -> Self {
            let env = Env::default();
            env.mock_all_auths();
            // Disable the budget so setup calls don't count toward measurements
            env.cost_estimate().budget().reset_unlimited();

            let admin = Address::generate(&env);
            let buyer = Address::generate(&env);
            let seller = Address::generate(&env);
            let treasury = Address::generate(&env);
            let mediator = Address::generate(&env);

            let contract_id = env.register(EscrowContract, ());
            let usdc_id = env
                .register_stellar_asset_contract_v2(admin.clone())
                .address();

            token::StellarAssetClient::new(&env, &usdc_id).mint(&buyer, &(amount * 10));

            let client = EscrowContractClient::new(&env, &contract_id);
            client.initialize(&admin, &usdc_id, &treasury, &100_u32);
            client.set_mediator(&mediator);

            Ctx { env, contract_id, buyer, seller, mediator }
        }

        fn client(&self) -> EscrowContractClient<'_> {
            EscrowContractClient::new(&self.env, &self.contract_id)
        }

        /// Reset the budget to the default bounded Soroban budget, run `f`,
        /// then return (cpu_insns, mem_bytes).
        ///
        /// Setup calls use `reset_unlimited()` so they cannot fail while
        /// building test fixtures. Measured calls must restore default limits
        /// first; otherwise gas estimates can be recorded under an unrealistic
        /// unlimited budget and hide regressions.
        fn measure<F: FnOnce()>(&self, f: F) -> (u64, u64) {
            self.env.cost_estimate().budget().reset_default();
            f();
            let budget = self.env.cost_estimate().budget();
            let cpu = budget.cpu_instruction_cost();
            let mem = budget.memory_bytes_cost();
            assert!(cpu > 0, "gas measurement did not record CPU cost");
            assert!(mem > 0, "gas measurement did not record memory cost");
            (cpu, mem)
        }
    }

    // -----------------------------------------------------------------------
    // #388-1  create_trade hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_baselines_are_documented_and_bounded() {
        let baselines = [
            ("create_trade", BASELINE_CREATE_TRADE_CPU, BASELINE_CREATE_TRADE_MEM),
            ("deposit", BASELINE_DEPOSIT_CPU, BASELINE_DEPOSIT_MEM),
            ("initiate_dispute", BASELINE_DISPUTE_CPU, BASELINE_DISPUTE_MEM),
            ("resolve_dispute", BASELINE_RESOLVE_CPU, BASELINE_RESOLVE_MEM),
        ];

        for (name, cpu, mem) in baselines {
            assert!(cpu > 0, "{name} CPU baseline must be documented");
            assert!(mem > 0, "{name} memory baseline must be documented");
        }

        let (combined_cpu, combined_mem) = combined_lifecycle_baseline();
        assert!(combined_cpu >= BASELINE_RESOLVE_CPU);
        assert!(combined_mem >= BASELINE_RESOLVE_MEM);
    }

    // -----------------------------------------------------------------------
    // #388-2  create_trade hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_create_trade() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();

        let (cpu, mem) = ctx.measure(|| {
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
        });

        assert!(
            cpu <= BASELINE_CREATE_TRADE_CPU,
            "create_trade CPU regression: {cpu} > baseline {BASELINE_CREATE_TRADE_CPU}"
        );
        assert!(
            mem <= BASELINE_CREATE_TRADE_MEM,
            "create_trade MEM regression: {mem} > baseline {BASELINE_CREATE_TRADE_MEM}"
        );
    }

    // -----------------------------------------------------------------------
    // #388-3  deposit hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_deposit() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();
        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);

        let (cpu, mem) = ctx.measure(|| {
            client.deposit(&trade_id);
        });

        assert!(
            cpu <= BASELINE_DEPOSIT_CPU,
            "deposit CPU regression: {cpu} > baseline {BASELINE_DEPOSIT_CPU}"
        );
        assert!(
            mem <= BASELINE_DEPOSIT_MEM,
            "deposit MEM regression: {mem} > baseline {BASELINE_DEPOSIT_MEM}"
        );
    }

    // -----------------------------------------------------------------------
    // #388-4  initiate_dispute hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_initiate_dispute() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();
        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
        client.deposit(&trade_id);

        let (cpu, mem) = ctx.measure(|| {
            client.initiate_dispute(
                &trade_id,
                &ctx.buyer,
                &String::from_str(&ctx.env, "QmGasTestReason"),
            );
        });

        assert!(
            cpu <= BASELINE_DISPUTE_CPU,
            "initiate_dispute CPU regression: {cpu} > baseline {BASELINE_DISPUTE_CPU}"
        );
        assert!(
            mem <= BASELINE_DISPUTE_MEM,
            "initiate_dispute MEM regression: {mem} > baseline {BASELINE_DISPUTE_MEM}"
        );
    }

    // -----------------------------------------------------------------------
    // #388-5  resolve_dispute hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_resolve_dispute() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();
        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
        client.deposit(&trade_id);
        client.initiate_dispute(
            &trade_id,
            &ctx.buyer,
            &String::from_str(&ctx.env, "QmGasTestReason"),
        );

        let (cpu, mem) = ctx.measure(|| {
            client.resolve_dispute(&trade_id, &ctx.mediator, &5_000_u32);
        });

        assert!(
            cpu <= BASELINE_RESOLVE_CPU,
            "resolve_dispute CPU regression: {cpu} > baseline {BASELINE_RESOLVE_CPU}"
        );
        assert!(
            mem <= BASELINE_RESOLVE_MEM,
            "resolve_dispute MEM regression: {mem} > baseline {BASELINE_RESOLVE_MEM}"
        );
    }

    // -----------------------------------------------------------------------
    // #388-6  Regression guard: all four hot paths in sequence
    //         Ensures no cumulative footprint surprise across a full lifecycle.
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_full_dispute_lifecycle_combined() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();

        // Measure the entire dispute lifecycle as one unit
        let (cpu, mem) = ctx.measure(|| {
            let trade_id = client.create_trade(
                &ctx.buyer,
                &ctx.seller,
                &10_000_i128,
                &5000_u32,
                &5000_u32,
            );
            client.deposit(&trade_id);
            client.initiate_dispute(
                &trade_id,
                &ctx.buyer,
                &String::from_str(&ctx.env, "QmCombinedReason"),
            );
            client.resolve_dispute(&trade_id, &ctx.mediator, &5_000_u32);
        });

        // Combined threshold = checked sum of individual baselines.
        let (combined_cpu, combined_mem) = combined_lifecycle_baseline();

        assert!(
            cpu <= combined_cpu,
            "combined lifecycle CPU regression: {cpu} > combined baseline {combined_cpu}"
        );
        assert!(
            mem <= combined_mem,
            "combined lifecycle MEM regression: {mem} > combined baseline {combined_mem}"
        );
    }
}
