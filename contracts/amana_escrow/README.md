# amana_escrow

This crate contains the Soroban escrow contract used by Amana.

## cNGN migration and upgrade notes

### Migration behavior

- The contract already supports any Stellar token contract address passed to `initialize(admin, usdc_contract, treasury, fee_bps)`.
- For backward-compatibility, the storage key name remains `DataKey::UsdcContract`.
- Trades are token-bound at creation time (`Trade.token`), so existing trades keep their original token address and settlement path.
- The contract is single-initialize; it does not support in-place token switching after initialization.

### Storage compatibility contract

For production upgrades, these compatibility expectations must remain stable:

- `DataKey` variants and serialized layout remain unchanged, especially:
  - `UsdcContract`
  - `Trade(u64)`
  - `Mediator` and `MediatorRegistry(Address)`
  - `DisputeData(u64)`, `EvidenceList(u64)`, `VideoProof(u64)`, `Manifest(u64)`
- `initialize` remains one-time and rejects reinitialization.
- Legacy mediator compatibility is preserved:
  - `set_mediator()` legacy slot continues to interoperate with `add_mediator()` registry entries.
  - `remove_mediator()` continues clearing both legacy and registry paths when applicable.
- Legacy evidence accessor compatibility is preserved:
  - `get_evidence_list()` is the primary API.
  - `get_evidence()` remains available for legacy clients.

### Safe rollout guidance for cNGN production

1. Validate in staging using the same contract build and cNGN contract ID intended for production.
2. Run full contract test suite and verify lifecycle invariants before deployment.
3. Deploy upgraded WASM without renaming storage keys or changing `DataKey` ordering/serialization.
4. Initialize new production deployment with cNGN token contract.
5. Monitor event processing and settlement balances across:
   - new cNGN trades,
   - pre-existing trades bound to their original token.
6. Do not assume rollback can mutate already-initialized on-chain token configuration.

## Migration test checklist

Existing tests in `src/lib.rs` and `tests/dispute_flow.rs` cover migration-sensitive behavior:

- lifecycle continuity, invalid transitions, and conservation checks
- legacy + registry mediator interoperability and revocation semantics
- evidence/video/manifest compatibility and persistence guarantees
- long-ledger-gap continuity (`test_trade_id_counter_survives_long_ledger_gap`)

Before production rollout, execute:

```bash
cargo test
```

## Gas estimation and regression checks

Gas regression coverage lives in `src/tests/gas_footprint_tests.rs`. The suite
measures the main escrow hot paths:

- `create_trade`
- `deposit`
- `initiate_dispute`
- `resolve_dispute`
- the combined dispute lifecycle

The helper resets Soroban metering before each measured operation and records a
CPU and memory delta for that operation only. Setup calls such as contract
registration, token minting, initialization, and mediator registration are not
part of the reported sample. This keeps the test focused on user-facing
contract calls and avoids inflated estimates from fixture setup.

Run the gas regression checks from the `contracts` workspace:

```bash
cargo test -p amana_escrow gas_footprint_tests -- --nocapture
```

Run the full contract validation suite before deployment or after changing
shared contract behavior:

```bash
cargo test -p amana_escrow
```

### Baseline policy

Baseline constants in `gas_footprint_tests.rs` are versioned with the contract
tests and should only change when a contract change intentionally affects gas.
When re-baselining:

1. Run the gas test command on a clean checkout with the same Rust toolchain and
   Soroban SDK version used by CI.
2. Compare the new CPU and memory samples with the committed baselines.
3. Increase thresholds only enough to account for the intentional behavior
   change plus stable SDK variance.
4. Commit the contract change, baseline update, and explanation together.

Do not add network calls, live RPC dependencies, or time-sensitive assertions to
gas tests. They must remain deterministic unit tests so CI failures point to a
contract or SDK cost change rather than external infrastructure.

### Estimation assumptions

- Measurements use `Env::cost_estimate()` under Soroban test utilities.
- The reported values are regression signals for CI, not production fee quotes.
- Each measured sample must represent only the operation under test.
- Idle samples should remain zero; a non-zero idle sample means setup or
  bookkeeping has leaked into the estimate.
- Operation samples should remain non-zero; a zero sample means the estimator is
  no longer observing Soroban budget consumption.
