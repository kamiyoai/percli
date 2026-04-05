use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{EngineSnapshot, NamedEngine, POS_SCALE};

use super::types::*;

pub struct RunOptions {
    pub check_conservation: bool,
    pub step_by_step: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub final_snapshot: EngineSnapshot,
    pub step_results: Vec<StepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_num: usize,
    pub description: String,
    pub outcome: StepOutcome,
    pub snapshot: Option<EngineSnapshot>,
    pub delta: Option<DeltaSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepOutcome {
    Ok,
    Warning(String),
    QueryResult(EngineSnapshot),
    AssertPassed,
    AssertFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaSnapshot {
    pub vault_delta: i128,
    pub insurance_delta: i128,
    pub account_deltas: Vec<AccountDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDelta {
    pub name: String,
    pub capital_delta: i128,
    pub equity_maint_delta: i128,
}

pub fn run_scenario(scenario: &Scenario, opts: &RunOptions) -> Result<RunResult> {
    let params = scenario.params.to_risk_params();
    let mut engine = NamedEngine::new(
        params,
        scenario.market.initial_slot,
        scenario.market.initial_oracle_price,
    );

    let mut step_results = Vec::new();

    for (i, step) in scenario.steps.iter().enumerate() {
        let step_num = i + 1;

        let before = if opts.verbose {
            Some(engine.snapshot())
        } else {
            None
        };

        let mut result = execute_step(&mut engine, step, step_num)?;

        if opts.check_conservation {
            let snap = engine.snapshot();
            if !snap.conservation {
                bail!("Conservation check FAILED after step {step_num}");
            }
        }

        if opts.step_by_step {
            result.snapshot = Some(engine.snapshot());
        }

        if let Some(before) = before {
            let after = engine.snapshot();
            result.delta = Some(compute_delta(&before, &after));
        }

        step_results.push(result);
    }

    let final_snapshot = engine.snapshot();
    Ok(RunResult {
        final_snapshot,
        step_results,
    })
}

fn execute_step(engine: &mut NamedEngine, step: &Step, step_num: usize) -> Result<StepResult> {
    match step {
        Step::Deposit {
            account,
            amount,
            comment,
        } => {
            let desc = format_desc("deposit", comment, &format!("{account} {amount}"));
            engine
                .deposit(account, *amount as u128)
                .with_context(|| format!("step {step_num}: deposit {account} {amount}"))?;
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::Withdraw {
            account,
            amount,
            comment,
        } => {
            let desc = format_desc("withdraw", comment, &format!("{account} {amount}"));
            engine
                .withdraw(account, *amount as u128)
                .with_context(|| format!("step {step_num}: withdraw {account} {amount}"))?;
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::Trade {
            long,
            short,
            size,
            price,
            comment,
        } => {
            let size_q = (*size as i128)
                .checked_mul(POS_SCALE as i128)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "step {step_num}: size {size} overflows when scaled by POS_SCALE"
                    )
                })?;
            let desc = format_desc(
                "trade",
                comment,
                &format!("{long} long / {short} short x {size} @ {price}"),
            );
            engine
                .trade(long, short, size_q, *price)
                .with_context(|| format!("step {step_num}: trade {long}/{short}"))?;
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::Crank {
            oracle_price,
            slot,
            comment,
        } => {
            let desc = format_desc(
                "crank",
                comment,
                &format!("oracle={oracle_price} slot={slot}"),
            );
            engine
                .crank(*oracle_price, *slot)
                .with_context(|| format!("step {step_num}: crank"))?;

            let snap = engine.snapshot();
            let mut warnings = Vec::new();
            for acct in &snap.accounts {
                if !acct.above_maintenance_margin && acct.effective_position_q != 0 {
                    warnings.push(format!("{} below maintenance margin", acct.name));
                }
            }

            let outcome = if warnings.is_empty() {
                StepOutcome::Ok
            } else {
                StepOutcome::Warning(warnings.join("; "))
            };

            Ok(StepResult {
                step_num,
                description: desc,
                outcome,
                snapshot: None,
                delta: None,
            })
        }

        Step::Liquidate { account, comment } => {
            let desc = format_desc("liquidate", comment, account);
            let did_liq = engine
                .liquidate(account)
                .with_context(|| format!("step {step_num}: liquidate {account}"))?;
            let outcome = if did_liq {
                StepOutcome::Ok
            } else {
                StepOutcome::Warning(format!("{account} was not liquidatable"))
            };
            Ok(StepResult {
                step_num,
                description: desc,
                outcome,
                snapshot: None,
                delta: None,
            })
        }

        Step::Settle { account, comment } => {
            let desc = format_desc("settle", comment, account);
            engine
                .settle(account)
                .with_context(|| format!("step {step_num}: settle {account}"))?;
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::SetOracle {
            oracle_price,
            comment,
        } => {
            let desc = format_desc("set_oracle", comment, &oracle_price.to_string());
            engine
                .set_oracle(*oracle_price)
                .with_context(|| format!("step {step_num}: set_oracle {oracle_price}"))?;
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::SetSlot { slot, comment } => {
            let desc = format_desc("set_slot", comment, &slot.to_string());
            engine.set_slot(*slot);
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::SetFundingRate { rate, comment } => {
            let desc = format_desc("set_funding_rate", comment, &rate.to_string());
            engine.set_funding_rate(*rate);
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::Ok,
                snapshot: None,
                delta: None,
            })
        }

        Step::Query { metric, comment } => {
            let snap = engine.snapshot();
            let metric_str = format!("{metric:?}");
            let desc = format_desc("query", comment, &metric_str);
            Ok(StepResult {
                step_num,
                description: desc,
                outcome: StepOutcome::QueryResult(snap.clone()),
                snapshot: Some(snap),
                delta: None,
            })
        }

        Step::Assert { condition, comment } => {
            let snap = engine.snapshot();
            let cond_str = format!("{condition:?}");
            let desc = format_desc("assert", comment, &cond_str);
            let outcome = check_assert(&snap, condition);
            Ok(StepResult {
                step_num,
                description: desc,
                outcome,
                snapshot: None,
                delta: None,
            })
        }
    }
}

fn check_assert(snap: &EngineSnapshot, condition: &AssertCondition) -> StepOutcome {
    match condition {
        AssertCondition::Conservation => {
            if snap.conservation {
                StepOutcome::AssertPassed
            } else {
                StepOutcome::AssertFailed("conservation check failed".to_string())
            }
        }
        AssertCondition::HaircutBelow { threshold } => {
            let h = snap.haircut_ratio_f64();
            if h < *threshold {
                StepOutcome::AssertPassed
            } else {
                StepOutcome::AssertFailed(format!("haircut {h:.4} >= {threshold}"))
            }
        }
        AssertCondition::AboveMaintenanceMargin { account } => {
            if let Some(acct) = snap.accounts.iter().find(|a| a.name == *account) {
                if acct.above_maintenance_margin {
                    StepOutcome::AssertPassed
                } else {
                    StepOutcome::AssertFailed(format!("{account} below maintenance margin"))
                }
            } else {
                StepOutcome::AssertFailed(format!("account {account} not found"))
            }
        }
        AssertCondition::BelowMaintenanceMargin { account } => {
            if let Some(acct) = snap.accounts.iter().find(|a| a.name == *account) {
                if !acct.above_maintenance_margin {
                    StepOutcome::AssertPassed
                } else {
                    StepOutcome::AssertFailed(format!("{account} still above maintenance margin"))
                }
            } else {
                StepOutcome::AssertFailed(format!("account {account} not found"))
            }
        }
        AssertCondition::AboveInitialMargin { account } => {
            if let Some(acct) = snap.accounts.iter().find(|a| a.name == *account) {
                if acct.above_initial_margin {
                    StepOutcome::AssertPassed
                } else {
                    StepOutcome::AssertFailed(format!("{account} below initial margin"))
                }
            } else {
                StepOutcome::AssertFailed(format!("account {account} not found"))
            }
        }
        AssertCondition::BelowInitialMargin { account } => {
            if let Some(acct) = snap.accounts.iter().find(|a| a.name == *account) {
                if !acct.above_initial_margin {
                    StepOutcome::AssertPassed
                } else {
                    StepOutcome::AssertFailed(format!("{account} still above initial margin"))
                }
            } else {
                StepOutcome::AssertFailed(format!("account {account} not found"))
            }
        }
        AssertCondition::VaultAbove { amount } => {
            if snap.vault > *amount {
                StepOutcome::AssertPassed
            } else {
                StepOutcome::AssertFailed(format!("vault {} is not above {amount}", snap.vault))
            }
        }
        AssertCondition::VaultBelow { amount } => {
            if snap.vault < *amount {
                StepOutcome::AssertPassed
            } else {
                StepOutcome::AssertFailed(format!("vault {} is not below {amount}", snap.vault))
            }
        }
    }
}

fn compute_delta(before: &EngineSnapshot, after: &EngineSnapshot) -> DeltaSnapshot {
    let vault_delta = after.vault as i128 - before.vault as i128;
    let insurance_delta = after.insurance_fund as i128 - before.insurance_fund as i128;

    let mut account_deltas = Vec::new();
    for acct_after in &after.accounts {
        let before_acct = before.accounts.iter().find(|a| a.name == acct_after.name);
        let (cap_before, eq_before) = before_acct
            .map(|a| (a.capital as i128, a.equity_maint))
            .unwrap_or((0, 0));

        let cap_delta = acct_after.capital as i128 - cap_before;
        let eq_delta = acct_after.equity_maint - eq_before;

        if cap_delta != 0 || eq_delta != 0 {
            account_deltas.push(AccountDelta {
                name: acct_after.name.clone(),
                capital_delta: cap_delta,
                equity_maint_delta: eq_delta,
            });
        }
    }

    DeltaSnapshot {
        vault_delta,
        insurance_delta,
        account_deltas,
    }
}

fn format_desc(action: &str, comment: &Option<String>, detail: &str) -> String {
    match comment {
        Some(c) => format!("{action} {detail} — {c}"),
        None => format!("{action} {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_scenario(steps: Vec<Step>) -> Scenario {
        Scenario {
            meta: Meta::default(),
            params: crate::ParamsConfig::default(),
            market: MarketConfig::default(),
            steps,
        }
    }

    fn default_opts() -> RunOptions {
        RunOptions {
            check_conservation: true,
            step_by_step: false,
            verbose: false,
        }
    }

    #[test]
    fn trade_size_overflow_is_checked() {
        // i64::MAX * POS_SCALE fits in i128, so verify checked_mul doesn't panic
        // and the engine properly handles it (rejects on margin/size grounds)
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 1_000_000,
                comment: None,
            },
            Step::Deposit {
                account: "b".into(),
                amount: 1_000_000,
                comment: None,
            },
            Step::Crank {
                oracle_price: 1000,
                slot: 1,
                comment: None,
            },
            Step::Trade {
                long: "a".into(),
                short: "b".into(),
                size: i64::MAX,
                price: 1000,
                comment: None,
            },
        ]);
        // Should fail (either overflow or engine rejection) but not panic
        assert!(run_scenario(&scenario, &default_opts()).is_err());
    }

    #[test]
    fn assert_conservation_pass() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 1000,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::Conservation,
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[1].outcome,
            StepOutcome::AssertPassed
        ));
    }

    #[test]
    fn assert_vault_above_pass() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 5000,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::VaultAbove { amount: 1000 },
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[1].outcome,
            StepOutcome::AssertPassed
        ));
    }

    #[test]
    fn assert_vault_above_fail() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 500,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::VaultAbove { amount: 1000 },
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[1].outcome,
            StepOutcome::AssertFailed(_)
        ));
    }

    #[test]
    fn assert_vault_below_pass() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 500,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::VaultBelow { amount: 1000 },
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[1].outcome,
            StepOutcome::AssertPassed
        ));
    }

    #[test]
    fn assert_vault_below_fail() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 5000,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::VaultBelow { amount: 1000 },
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[1].outcome,
            StepOutcome::AssertFailed(_)
        ));
    }

    #[test]
    fn assert_above_initial_margin_pass() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 100_000,
                comment: None,
            },
            Step::Deposit {
                account: "b".into(),
                amount: 100_000,
                comment: None,
            },
            Step::Crank {
                oracle_price: 1000,
                slot: 1,
                comment: None,
            },
            Step::Trade {
                long: "a".into(),
                short: "b".into(),
                size: 10,
                price: 1000,
                comment: None,
            },
            Step::Assert {
                condition: AssertCondition::AboveInitialMargin {
                    account: "a".into(),
                },
                comment: None,
            },
        ]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[4].outcome,
            StepOutcome::AssertPassed
        ));
    }

    #[test]
    fn assert_below_initial_margin_unknown_account() {
        let scenario = basic_scenario(vec![Step::Assert {
            condition: AssertCondition::BelowInitialMargin {
                account: "ghost".into(),
            },
            comment: None,
        }]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[0].outcome,
            StepOutcome::AssertFailed(_)
        ));
    }

    #[test]
    fn assert_above_maintenance_margin_unknown_account() {
        let scenario = basic_scenario(vec![Step::Assert {
            condition: AssertCondition::AboveMaintenanceMargin {
                account: "ghost".into(),
            },
            comment: None,
        }]);
        let result = run_scenario(&scenario, &default_opts()).unwrap();
        assert!(matches!(
            result.step_results[0].outcome,
            StepOutcome::AssertFailed(_)
        ));
    }

    #[test]
    fn verbose_produces_deltas() {
        let scenario = basic_scenario(vec![
            Step::Deposit {
                account: "a".into(),
                amount: 1000,
                comment: None,
            },
            Step::Deposit {
                account: "a".into(),
                amount: 2000,
                comment: None,
            },
        ]);
        let opts = RunOptions {
            check_conservation: true,
            step_by_step: false,
            verbose: true,
        };
        let result = run_scenario(&scenario, &opts).unwrap();
        assert!(result.step_results[0].delta.is_some());
        let delta = result.step_results[1].delta.as_ref().unwrap();
        assert_eq!(delta.vault_delta, 2000);
    }

    #[test]
    fn step_by_step_produces_snapshots() {
        let scenario = basic_scenario(vec![Step::Deposit {
            account: "a".into(),
            amount: 1000,
            comment: None,
        }]);
        let opts = RunOptions {
            check_conservation: true,
            step_by_step: true,
            verbose: false,
        };
        let result = run_scenario(&scenario, &opts).unwrap();
        assert!(result.step_results[0].snapshot.is_some());
    }
}
