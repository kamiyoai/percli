use anyhow::{bail, Result};
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, InitializeMarketArgs, RiskParamsInput};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    mint: &Pubkey,
    oracle: &Pubkey,
    matcher: &Pubkey,
    init_oracle_price: u64,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _bump) = config.market_pda();
    let (vault_pda, _) = config.vault_pda();

    if rpc.account_exists(&market_pda)? {
        bail!("Market already exists at {market_pda}");
    }

    let slot = rpc.get_slot()?;

    // Default risk params for devnet testing
    let params = RiskParamsInput {
        warmup_period_slots: 0,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 4096,
        new_account_fee: 0,
        maintenance_fee_per_slot: 0,
        max_crank_staleness_slots: 100,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: 1_000_000,
        min_liquidation_abs: 100,
        min_initial_deposit: 1000,
        min_nonzero_mm_req: 100,
        min_nonzero_im_req: 200,
        insurance_floor: 0,
    };

    let args = InitializeMarketArgs {
        init_slot: slot,
        init_oracle_price,
        params,
    };

    let ix = ix::initialize_market_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        mint,
        &vault_pda,
        oracle,
        matcher,
        &spl_token::id(),
        args,
    );

    println!("Deploying market...");
    println!("  Authority: {}", config.authority());
    println!("  Market PDA: {market_pda}");
    println!("  Vault PDA:  {vault_pda}");
    println!("  Mint:       {mint}");
    println!("  Oracle:     {oracle}");
    println!("  Matcher:    {matcher}");
    println!("  RPC: {}", config.rpc_url);

    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    println!("Market deployed.");

    Ok(())
}
