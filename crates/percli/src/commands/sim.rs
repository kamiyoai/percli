use anyhow::{Context, Result};

use crate::format::{self, status, OutputFormat};
use percli_core::scenario::{runner, Scenario};

pub fn run(
    path: &str,
    fmt: OutputFormat,
    step_by_step: bool,
    check_conservation: bool,
    verbose: bool,
    overrides: &[String],
) -> Result<()> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            status::error_with_help(
                &format!("could not read scenario file: {path}"),
                Some(path),
                Some("check the file path and try again"),
            );
            std::process::exit(1);
        }
    };

    let mut scenario: Scenario =
        toml::from_str(&content).with_context(|| format!("failed to parse scenario: {path}"))?;

    apply_overrides(&mut scenario, overrides)?;

    let step_count = scenario.steps.len();
    if !scenario.meta.name.is_empty() {
        status::status(
            "Simulating",
            &format!("{} ({} steps)", scenario.meta.name, step_count),
        );
        if !scenario.meta.description.is_empty() {
            eprintln!("{:>12} {}", "", scenario.meta.description);
        }
        eprintln!();
    } else {
        status::status("Simulating", &format!("{path} ({step_count} steps)"));
        eprintln!();
    }

    let opts = runner::RunOptions {
        check_conservation,
        step_by_step,
        verbose,
    };

    let result = runner::run_scenario(&scenario, &opts)?;

    format::print_run_result(&result, fmt)?;

    let failures: Vec<_> = result
        .step_results
        .iter()
        .filter(|sr| matches!(sr.outcome, runner::StepOutcome::AssertFailed(_)))
        .collect();

    if !failures.is_empty() {
        eprintln!();
        for f in &failures {
            if let runner::StepOutcome::AssertFailed(msg) = &f.outcome {
                status::error(&format!("assertion failed at step {}: {msg}", f.step_num));
            }
        }
        std::process::exit(1);
    }

    Ok(())
}

fn apply_overrides(scenario: &mut Scenario, overrides: &[String]) -> Result<()> {
    for ov in overrides {
        let (key, val) = ov
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("override must be key=value, got: {ov}"))?;

        let p = &mut scenario.params;
        match key {
            "warmup_period_slots" => p.warmup_period_slots = val.parse()?,
            "maintenance_margin_bps" => p.maintenance_margin_bps = val.parse()?,
            "initial_margin_bps" => p.initial_margin_bps = val.parse()?,
            "trading_fee_bps" => p.trading_fee_bps = val.parse()?,
            "max_accounts" => p.max_accounts = val.parse()?,
            "new_account_fee" => p.new_account_fee = val.parse()?,
            "maintenance_fee_per_slot" => p.maintenance_fee_per_slot = val.parse()?,
            "max_crank_staleness_slots" => p.max_crank_staleness_slots = val.parse()?,
            "liquidation_fee_bps" => p.liquidation_fee_bps = val.parse()?,
            "liquidation_fee_cap" => p.liquidation_fee_cap = val.parse()?,
            "min_liquidation_abs" => p.min_liquidation_abs = val.parse()?,
            "min_initial_deposit" => p.min_initial_deposit = val.parse()?,
            "min_nonzero_mm_req" => p.min_nonzero_mm_req = val.parse()?,
            "min_nonzero_im_req" => p.min_nonzero_im_req = val.parse()?,
            "insurance_floor" => p.insurance_floor = val.parse()?,
            _ => anyhow::bail!("unknown parameter: {key}"),
        }
    }
    Ok(())
}
