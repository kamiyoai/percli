use anyhow::{bail, Result};

use crate::config::ChainConfig;
use crate::read;
use crate::rpc::ChainRpc;

pub enum QueryTarget {
    Market,
    Account(u16),
}

pub fn run(config: &ChainConfig, target: QueryTarget) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let data = rpc.get_account_data(&market_pda)?;
    let engine = read::engine_from_data(&data)?;

    match target {
        QueryTarget::Market => print_market(engine, &market_pda, config),
        QueryTarget::Account(idx) => print_account(engine, idx),
    }
}

fn print_market(
    engine: &percli_core::RiskEngine,
    market_pda: &solana_sdk::pubkey::Pubkey,
    config: &ChainConfig,
) -> Result<()> {
    let vault = engine.vault.get();
    let insurance = engine.insurance_fund.balance.get();
    let (h_num, h_den) = engine.haircut_ratio();
    let conservation = engine.check_conservation();

    println!("Market: {market_pda}");
    println!("  Program:      {}", config.program_id);
    println!("  Authority:    {}", config.authority());
    println!("  Vault:        {vault}");
    println!("  Insurance:    {insurance}");
    println!("  Haircut:      {h_num}/{h_den}");
    println!(
        "  Conservation: {}",
        if conservation { "PASS" } else { "FAIL" }
    );
    println!("  Oracle price: {}", engine.last_oracle_price);
    println!("  Last slot:    {}", engine.last_crank_slot);

    // Count active accounts
    let mut active = 0u32;
    for i in 0..engine.accounts.len() {
        if engine.is_used(i) {
            active += 1;
        }
    }
    println!("  Accounts:     {active}/{}", engine.accounts.len());

    Ok(())
}

fn print_account(engine: &percli_core::RiskEngine, idx: u16) -> Result<()> {
    let i = idx as usize;
    if i >= engine.accounts.len() {
        bail!(
            "Account index {idx} out of range (max {})",
            engine.accounts.len() - 1
        );
    }
    if !engine.is_used(i) {
        bail!("Account slot {idx} is empty");
    }

    let acct = &engine.accounts[i];
    let eff_pos = engine.effective_pos_q(i);
    let equity_maint = engine.account_equity_maint_raw(acct);
    let equity_init = engine.account_equity_init_raw(acct, i);
    let above_mm = engine.is_above_maintenance_margin(acct, i, engine.last_oracle_price);
    let above_im = engine.is_above_initial_margin(acct, i, engine.last_oracle_price);
    let notional = engine.notional(i, engine.last_oracle_price);

    println!("Account #{idx}");
    println!("  Capital:    {}", acct.capital.get());
    println!("  PnL:        {}", acct.pnl);
    println!("  Reserved:   {}", acct.reserved_pnl);
    println!("  Position:   {eff_pos}");
    println!("  Notional:   {notional}");
    println!("  Equity(M):  {equity_maint}");
    println!("  Equity(I):  {equity_init}");
    println!("  Above MM:   {above_mm}");
    println!("  Above IM:   {above_im}");

    Ok(())
}
