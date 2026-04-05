use anyhow::{bail, Result};
use std::path::Path;

use super::state;
use crate::format::{self, status, OutputFormat};

pub fn run(state_path: &str, metric: &str, account: Option<&str>, fmt: OutputFormat) -> Result<()> {
    let path = Path::new(state_path);
    let saved = state::load_or_create(path)?;
    if !path.exists() {
        bail!("state file not found: {state_path}");
    }

    status::status_cyan("Querying", metric);

    let engine = state::replay(&saved)?;
    let snap = engine.snapshot();

    match metric {
        "summary" | "all" => {
            format::print_snapshot(&snap, fmt)?;
        }
        "haircut" => {
            let h = snap.haircut_ratio_f64();
            match fmt {
                OutputFormat::Table => {
                    println!("Haircut ratio: {h:.6}");
                    println!("  numerator:   {}", snap.haircut_numerator);
                    println!("  denominator: {}", snap.haircut_denominator);
                }
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "haircut_ratio": h,
                            "numerator": snap.haircut_numerator,
                            "denominator": snap.haircut_denominator,
                        })
                    );
                }
            }
        }
        "conservation" => {
            match fmt {
                OutputFormat::Table => {
                    println!(
                        "Conservation: {}",
                        if snap.conservation { "PASS" } else { "FAIL" }
                    );
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::json!({ "conservation": snap.conservation }));
                }
            }
        }
        "vault" => {
            match fmt {
                OutputFormat::Table => {
                    println!("Vault:     {}", snap.vault);
                    println!("Insurance: {}", snap.insurance_fund);
                }
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "vault": snap.vault,
                            "insurance_fund": snap.insurance_fund,
                        })
                    );
                }
            }
        }
        "equity" | "margin" | "position" => {
            let acct_name = account.ok_or_else(|| {
                anyhow::anyhow!("{metric} query requires --account <name>")
            })?;
            let acct = snap
                .accounts
                .iter()
                .find(|a| a.name == acct_name)
                .ok_or_else(|| anyhow::anyhow!("account not found: {acct_name}"))?;

            match fmt {
                OutputFormat::Table => {
                    println!("Account: {}", acct.name);
                    println!("  Capital:    {}", acct.capital);
                    println!("  PnL:        {}", acct.pnl);
                    println!("  Reserved:   {}", acct.reserved_pnl);
                    println!("  Equity(M):  {}", acct.equity_maint);
                    println!("  Equity(I):  {}", acct.equity_init);
                    println!("  Position:   {}", acct.effective_position_q);
                    println!("  Notional:   {}", acct.notional);
                    println!("  Above MM:   {}", acct.above_maintenance_margin);
                    println!("  Above IM:   {}", acct.above_initial_margin);
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(acct)?);
                }
            }
        }
        "accounts" => {
            match fmt {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&snap.accounts)?);
                }
                OutputFormat::Table => {
                    format::print_snapshot(&snap, fmt)?;
                }
            }
        }
        other => bail!("unknown metric: {other}. Available: summary, haircut, conservation, vault, equity, margin, position, accounts"),
    }

    Ok(())
}
