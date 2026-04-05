// Property-based fuzz tests for the Percolator RiskEngine.
//
// These tests complement the Kani formal verification proofs by covering
// code paths that are intractable for CBMC (U512 wide division in trades)
// and multi-step operation sequences.

use percli_core::{LiquidationPolicy, RiskEngine, RiskParams, MAX_ACCOUNTS, POS_SCALE, U128};
use proptest::prelude::*;

/// Minimal valid RiskParams for property tests: zero fees, zero warmup.
fn test_params() -> RiskParams {
    RiskParams {
        warmup_period_slots: 0,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 0,
        max_accounts: MAX_ACCOUNTS as u64,
        new_account_fee: U128::ZERO,
        maintenance_fee_per_slot: U128::ZERO,
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 0,
        liquidation_fee_cap: U128::ZERO,
        min_liquidation_abs: U128::ZERO,
        min_initial_deposit: U128::new(3),
        min_nonzero_mm_req: 1,
        min_nonzero_im_req: 2,
        insurance_floor: U128::ZERO,
    }
}

/// RiskParams with nonzero trading fee for trade tests.
fn trade_params() -> RiskParams {
    let mut p = test_params();
    p.trading_fee_bps = 10; // 0.1%
    p
}

proptest! {
    /// deposit(x) then withdraw(x) preserves conservation invariant.
    #[test]
    fn deposit_withdraw_roundtrip(
        oracle in 1u64..1_000_000u64,
        cap in 100u128..1_000_000_000u128,
    ) {
        let mut eng = RiskEngine::new_with_market(test_params(), 100, oracle);
        eng.deposit(0, cap, oracle, 100).unwrap();
        prop_assert!(eng.check_conservation());

        // Withdraw everything (may fail if below min_initial_deposit floor)
        if eng.withdraw_not_atomic(0, cap, oracle, 100, 0).is_ok() {
            prop_assert!(eng.check_conservation());
        }
    }

    /// N deposits into different accounts all preserve conservation.
    #[test]
    fn multi_deposit_conservation(
        oracle in 1u64..1_000_000u64,
        caps in proptest::collection::vec(100u128..500_000_000u128, 1..4),
    ) {
        let mut eng = RiskEngine::new_with_market(test_params(), 100, oracle);
        for (i, &cap) in caps.iter().enumerate() {
            if eng.deposit(i as u16, cap, oracle, 100).is_ok() {
                prop_assert!(eng.check_conservation());
            }
        }
    }

    /// Random deposit/withdraw/crank sequences never panic and conservation holds.
    #[test]
    fn random_ops_never_panic(
        oracle in 1u64..1_000_000u64,
        ops in proptest::collection::vec(
            prop_oneof![
                (0u16..4, 100u128..1_000_000u128).prop_map(|(i, c)| (0u8, i, c)),
                (0u16..4, 1u128..500_000u128).prop_map(|(i, a)| (1u8, i, a)),
                Just((2u8, 0u16, 0u128)),
            ],
            1..20
        ),
    ) {
        let slot = 100u64;
        let mut eng = RiskEngine::new_with_market(test_params(), slot, oracle);
        for (op, idx, amount) in &ops {
            match op {
                0 => { let _ = eng.deposit(*idx, *amount, oracle, slot); }
                1 => { let _ = eng.withdraw_not_atomic(*idx, *amount, oracle, slot, 0); }
                _ => { let _ = eng.accrue_market_to(slot, oracle); }
            }
        }
        prop_assert!(eng.check_conservation());
    }

    /// haircut_ratio always returns h_num <= h_den.
    #[test]
    fn haircut_ratio_always_bounded(
        oracle in 1u64..1_000_000u64,
        caps in proptest::collection::vec(100u128..1_000_000_000u128, 1..4),
    ) {
        let mut eng = RiskEngine::new_with_market(test_params(), 100, oracle);
        for (i, &cap) in caps.iter().enumerate() {
            let _ = eng.deposit(i as u16, cap, oracle, 100);
        }
        let (h_num, h_den) = eng.haircut_ratio();
        if h_den > 0 {
            prop_assert!(h_num <= h_den);
        } else {
            prop_assert!(h_num == 1 && h_den == 1);
        }
    }

    /// mul_div_floor/ceil consistency: ceil >= floor and diff <= 1.
    #[test]
    fn wide_math_floor_ceil_consistency(
        a in 0u128..u64::MAX as u128,
        b in 0u128..u64::MAX as u128,
        d in 1u128..u64::MAX as u128,
    ) {
        if a.checked_mul(b).is_some() {
            let floor = percolator::wide_math::mul_div_floor_u128(a, b, d);
            let ceil = percolator::wide_math::mul_div_ceil_u128(a, b, d);
            prop_assert!(ceil >= floor);
            prop_assert!(ceil <= floor + 1);
        }
    }

    /// Conservation and OI balance after trades.
    /// This replaces the intractable Kani trade proof.
    #[test]
    fn trade_conservation(
        oracle in 100u64..100_000u64,
        cap_a in 1_000_000u128..100_000_000u128,
        cap_b in 1_000_000u128..100_000_000u128,
        trade_size in 1i128..10_000i128,
    ) {
        let slot = 100u64;
        let mut eng = RiskEngine::new_with_market(trade_params(), slot, oracle);
        eng.deposit(0, cap_a, oracle, slot).unwrap();
        eng.deposit(1, cap_b, oracle, slot).unwrap();
        prop_assert!(eng.check_conservation());

        let size_q = trade_size * (POS_SCALE as i128);
        if eng.execute_trade_not_atomic(0, 1, oracle, slot, size_q, oracle, 0).is_ok() {
            prop_assert!(eng.check_conservation());
            prop_assert!(eng.oi_eff_long_q == eng.oi_eff_short_q);
        }
    }

    /// Liquidation of accounts with open positions preserves conservation
    /// and OI balance. Covers the U512 path that Kani cannot reach.
    #[test]
    fn liquidate_with_position_conservation(
        oracle in 100u64..100_000u64,
        cap_a in 10_000_000u128..100_000_000u128,
        cap_b in 10_000_000u128..100_000_000u128,
        trade_size in 1i128..1_000i128,
        crash_pct in 10u64..80u64,
    ) {
        let slot = 100u64;
        let mut eng = RiskEngine::new_with_market(trade_params(), slot, oracle);
        eng.deposit(0, cap_a, oracle, slot).unwrap();
        eng.deposit(1, cap_b, oracle, slot).unwrap();

        let size_q = trade_size * (POS_SCALE as i128);
        if eng.execute_trade_not_atomic(0, 1, oracle, slot, size_q, oracle, 0).is_err() {
            return Ok(());
        }
        prop_assert!(eng.check_conservation());

        // Crash the oracle to make one side undercollateralized
        let crashed_oracle = oracle.saturating_sub(oracle * crash_pct / 100).max(1);
        let slot2 = slot + 10;

        // Try liquidating account 0 (the long) at crashed price
        let _ = eng.liquidate_at_oracle_not_atomic(
            0, slot2, crashed_oracle, LiquidationPolicy::FullClose, 0,
        );
        prop_assert!(eng.check_conservation());
        prop_assert!(eng.oi_eff_long_q == eng.oi_eff_short_q);
    }

    /// Multi-step sequences including trade, liquidate, settle, and crank.
    /// Exercises the full operation mix for robustness and conservation.
    #[test]
    fn full_ops_sequence_never_panic(
        oracle in 100u64..100_000u64,
        cap_a in 5_000_000u128..50_000_000u128,
        cap_b in 5_000_000u128..50_000_000u128,
        ops in proptest::collection::vec(
            prop_oneof![
                // trade
                (1i128..500i128).prop_map(|s| (0u8, s as u64)),
                // crank with oracle shift
                (80u64..120u64).prop_map(|pct| (1u8, pct)),
                // liquidate account 0
                Just((2u8, 0u64)),
                // liquidate account 1
                Just((3u8, 0u64)),
                // settle account 0
                Just((4u8, 0u64)),
                // deposit more to account 0
                (100_000u64..5_000_000u64).prop_map(|a| (5u8, a)),
            ],
            1..15
        ),
    ) {
        let slot = 100u64;
        let mut eng = RiskEngine::new_with_market(trade_params(), slot, oracle);
        eng.deposit(0, cap_a, oracle, slot).unwrap();
        eng.deposit(1, cap_b, oracle, slot).unwrap();

        let mut current_oracle = oracle;
        let mut current_slot = slot;

        for (op, val) in &ops {
            match op {
                0 => {
                    // Trade
                    let size_q = (*val as i128) * (POS_SCALE as i128);
                    let _ = eng.execute_trade_not_atomic(
                        0, 1, current_oracle, current_slot, size_q, current_oracle, 0,
                    );
                }
                1 => {
                    // Crank with oracle shift (val is percentage, 80-120%)
                    current_oracle = (current_oracle as u128 * *val as u128 / 100) as u64;
                    current_oracle = current_oracle.max(1);
                    current_slot += 10;
                    let _ = eng.accrue_market_to(current_slot, current_oracle);
                }
                2 => {
                    let _ = eng.liquidate_at_oracle_not_atomic(
                        0, current_slot, current_oracle,
                        LiquidationPolicy::FullClose, 0,
                    );
                }
                3 => {
                    let _ = eng.liquidate_at_oracle_not_atomic(
                        1, current_slot, current_oracle,
                        LiquidationPolicy::FullClose, 0,
                    );
                }
                4 => {
                    let _ = eng.settle_account_not_atomic(0, current_oracle, current_slot, 0);
                }
                _ => {
                    let _ = eng.deposit(0, *val as u128, current_oracle, current_slot);
                }
            }
        }
        prop_assert!(eng.check_conservation());
        prop_assert!(eng.oi_eff_long_q == eng.oi_eff_short_q);
    }

    /// Funding accrual with live OI preserves conservation.
    /// Exercises accrue_market_to with nonzero positions and oracle changes.
    #[test]
    fn funding_accrual_with_oi_conservation(
        oracle in 100u64..100_000u64,
        cap_a in 10_000_000u128..100_000_000u128,
        cap_b in 10_000_000u128..100_000_000u128,
        trade_size in 1i128..1_000i128,
        new_oracle_pct in 80u64..120u64,
        slot_advance in 1u64..1000u64,
    ) {
        let slot = 100u64;
        let mut eng = RiskEngine::new_with_market(trade_params(), slot, oracle);
        eng.deposit(0, cap_a, oracle, slot).unwrap();
        eng.deposit(1, cap_b, oracle, slot).unwrap();

        // Open a position to create nonzero OI
        let size_q = trade_size * (POS_SCALE as i128);
        if eng.execute_trade_not_atomic(0, 1, oracle, slot, size_q, oracle, 0).is_err() {
            return Ok(());
        }
        prop_assert!(eng.oi_eff_long_q > 0);
        prop_assert!(eng.check_conservation());

        // Advance with new oracle price (exercises funding sub-stepping)
        let new_oracle = (oracle as u128 * new_oracle_pct as u128 / 100) as u64;
        let new_oracle = new_oracle.max(1);
        let new_slot = slot + slot_advance;

        if eng.accrue_market_to(new_slot, new_oracle).is_ok() {
            prop_assert!(eng.check_conservation());
            prop_assert!(eng.oi_eff_long_q == eng.oi_eff_short_q);
        }
    }
}
