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
    use soroban_sdk::{Address, Env, String, testutils::Address as _, token};

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

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct GasSample {
        cpu: u64,
        mem: u64,
    }

    impl GasSample {
        fn current(env: &Env) -> Self {
            let budget = env.cost_estimate().budget();
            Self {
                cpu: budget.cpu_instruction_cost(),
                mem: budget.memory_bytes_cost(),
            }
        }

        fn delta_since(self, before: Self) -> Self {
            Self {
                cpu: self
                    .cpu
                    .checked_sub(before.cpu)
                    .expect("CPU budget counter moved backwards"),
                mem: self
                    .mem
                    .checked_sub(before.mem)
                    .expect("memory budget counter moved backwards"),
            }
        }
    }

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

            Ctx {
                env,
                contract_id,
                buyer,
                seller,
                mediator,
            }
        }

        fn client(&self) -> EscrowContractClient<'_> {
            EscrowContractClient::new(&self.env, &self.contract_id)
        }

        /// Reset metering, run `f`, then return only the cost delta for `f`.
        fn measure<F: FnOnce()>(&self, f: F) -> GasSample {
            self.env.cost_estimate().budget().reset_unlimited();
            let before = GasSample::current(&self.env);
            f();
            GasSample::current(&self.env).delta_since(before)
        }
    }

    // -----------------------------------------------------------------------
    // #388-1  create_trade hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_create_trade() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();

        let gas = ctx.measure(|| {
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
        });

        assert!(
            gas.cpu <= BASELINE_CREATE_TRADE_CPU,
            "create_trade CPU regression: {} > baseline {BASELINE_CREATE_TRADE_CPU}",
            gas.cpu
        );
        assert!(
            gas.mem <= BASELINE_CREATE_TRADE_MEM,
            "create_trade MEM regression: {} > baseline {BASELINE_CREATE_TRADE_MEM}",
            gas.mem
        );
    }

    // -----------------------------------------------------------------------
    // #388-2  deposit hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_deposit() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();
        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);

        let gas = ctx.measure(|| {
            client.deposit(&trade_id);
        });

        assert!(
            gas.cpu <= BASELINE_DEPOSIT_CPU,
            "deposit CPU regression: {} > baseline {BASELINE_DEPOSIT_CPU}",
            gas.cpu
        );
        assert!(
            gas.mem <= BASELINE_DEPOSIT_MEM,
            "deposit MEM regression: {} > baseline {BASELINE_DEPOSIT_MEM}",
            gas.mem
        );
    }

    // -----------------------------------------------------------------------
    // #388-3  initiate_dispute hot path
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_initiate_dispute() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();
        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
        client.deposit(&trade_id);

        let gas = ctx.measure(|| {
            client.initiate_dispute(
                &trade_id,
                &ctx.buyer,
                &String::from_str(&ctx.env, "QmGasTestReason"),
            );
        });

        assert!(
            gas.cpu <= BASELINE_DISPUTE_CPU,
            "initiate_dispute CPU regression: {} > baseline {BASELINE_DISPUTE_CPU}",
            gas.cpu
        );
        assert!(
            gas.mem <= BASELINE_DISPUTE_MEM,
            "initiate_dispute MEM regression: {} > baseline {BASELINE_DISPUTE_MEM}",
            gas.mem
        );
    }

    // -----------------------------------------------------------------------
    // #388-4  resolve_dispute hot path
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

        let gas = ctx.measure(|| {
            client.resolve_dispute(&trade_id, &ctx.mediator, &5_000_u32);
        });

        assert!(
            gas.cpu <= BASELINE_RESOLVE_CPU,
            "resolve_dispute CPU regression: {} > baseline {BASELINE_RESOLVE_CPU}",
            gas.cpu
        );
        assert!(
            gas.mem <= BASELINE_RESOLVE_MEM,
            "resolve_dispute MEM regression: {} > baseline {BASELINE_RESOLVE_MEM}",
            gas.mem
        );
    }

    // -----------------------------------------------------------------------
    // #388-5  Regression guard: all four hot paths in sequence
    //         Ensures no cumulative footprint surprise across a full lifecycle.
    // -----------------------------------------------------------------------
    #[test]
    fn test_gas_full_dispute_lifecycle_combined() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();

        // Measure the entire dispute lifecycle as one unit
        let gas = ctx.measure(|| {
            let trade_id =
                client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);
            client.deposit(&trade_id);
            client.initiate_dispute(
                &trade_id,
                &ctx.buyer,
                &String::from_str(&ctx.env, "QmCombinedReason"),
            );
            client.resolve_dispute(&trade_id, &ctx.mediator, &5_000_u32);
        });

        // Combined threshold = sum of individual baselines
        let combined_cpu = BASELINE_CREATE_TRADE_CPU
            + BASELINE_DEPOSIT_CPU
            + BASELINE_DISPUTE_CPU
            + BASELINE_RESOLVE_CPU;
        let combined_mem = BASELINE_CREATE_TRADE_MEM
            + BASELINE_DEPOSIT_MEM
            + BASELINE_DISPUTE_MEM
            + BASELINE_RESOLVE_MEM;

        assert!(
            gas.cpu <= combined_cpu,
            "combined lifecycle CPU regression: {} > combined baseline {combined_cpu}",
            gas.cpu
        );
        assert!(
            gas.mem <= combined_mem,
            "combined lifecycle MEM regression: {} > combined baseline {combined_mem}",
            gas.mem
        );
    }

    #[test]
    fn test_gas_estimator_reports_operation_delta_only() {
        let ctx = Ctx::new(10_000);
        let client = ctx.client();

        let trade_id =
            client.create_trade(&ctx.buyer, &ctx.seller, &10_000_i128, &5000_u32, &5000_u32);

        let idle = ctx.measure(|| {});
        assert_eq!(
            idle,
            GasSample { cpu: 0, mem: 0 },
            "idle estimator sample should not include setup or prior operation cost"
        );

        let deposit = ctx.measure(|| {
            client.deposit(&trade_id);
        });
        assert!(
            deposit.cpu > 0,
            "deposit gas sample should include measured CPU cost"
        );
        assert!(
            deposit.mem > 0,
            "deposit gas sample should include measured memory cost"
        );
    }
}
