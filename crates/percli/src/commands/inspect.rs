use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use percli_core::scenario::Scenario;
use std::collections::HashMap;

use crate::format::status;

pub fn run(path: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read scenario file: {path}"))?;

    let scenario: Scenario =
        toml::from_str(&content).with_context(|| format!("failed to parse scenario: {path}"))?;

    status::status("Inspecting", path);
    eprintln!();

    println!(
        "{:>12}  {}",
        "Scenario".if_supports_color(owo_colors::Stream::Stdout, |t| t.bold()),
        scenario.meta.name
    );
    if !scenario.meta.description.is_empty() {
        println!("{:>12}  {}", "", scenario.meta.description);
    }

    println!(
        "{:>12}  maintenance_margin={}bps initial_margin={}bps fee={}bps",
        "Params".if_supports_color(owo_colors::Stream::Stdout, |t| t.bold()),
        scenario.params.maintenance_margin_bps,
        scenario.params.initial_margin_bps,
        scenario.params.trading_fee_bps,
    );

    println!(
        "{:>12}  oracle={} slot={}",
        "Market".if_supports_color(owo_colors::Stream::Stdout, |t| t.bold()),
        scenario.market.initial_oracle_price,
        scenario.market.initial_slot,
    );

    // Count step types
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for step in &scenario.steps {
        let kind = match step {
            percli_core::scenario::Step::Deposit { .. } => "deposit",
            percli_core::scenario::Step::Withdraw { .. } => "withdraw",
            percli_core::scenario::Step::Trade { .. } => "trade",
            percli_core::scenario::Step::Crank { .. } => "crank",
            percli_core::scenario::Step::Liquidate { .. } => "liquidate",
            percli_core::scenario::Step::Settle { .. } => "settle",
            percli_core::scenario::Step::SetOracle { .. } => "set_oracle",
            percli_core::scenario::Step::SetSlot { .. } => "set_slot",
            percli_core::scenario::Step::SetFundingRate { .. } => "set_funding_rate",
            percli_core::scenario::Step::Query { .. } => "query",
            percli_core::scenario::Step::Assert { .. } => "assert",
        };
        *counts.entry(kind).or_default() += 1;
    }

    let breakdown: Vec<String> = counts.iter().map(|(k, v)| format!("{v} {k}")).collect();

    println!(
        "{:>12}  {} ({})",
        "Steps".if_supports_color(owo_colors::Stream::Stdout, |t| t.bold()),
        scenario.steps.len(),
        breakdown.join(", "),
    );

    println!(
        "{:>12}  {}",
        "Valid".if_supports_color(owo_colors::Stream::Stdout, |t| t.bold()),
        "\u{2713}".if_supports_color(owo_colors::Stream::Stdout, |t| t.green()),
    );

    // Keep this line for backward compatibility with tests
    println!();
    println!("Scenario is valid.");

    Ok(())
}
