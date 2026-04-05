use anyhow::{bail, Result};

use crate::config::ChainConfig;
use crate::ix::{self, InitializeMarketArgs, RiskParamsInput};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _bump) = config.market_pda();

    if rpc.account_exists(&market_pda)? {
        bail!("Market already exists at {market_pda}");
    }

    let slot = rpc.get_slot()?;

    // Default risk params for devnet testing
    let params = RiskParamsInput {
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        liquidation_fee_bps: 50,
        insurance_fee_bps: 25,
        adl_threshold_bps: 200,
        max_leverage_q: 20,
        min_order_size_q: 1,
        tick_size: 1,
        max_oi_imbalance_q: 1_000_000,
        haircut_start_bps: 100,
        haircut_end_bps: 500,
        haircut_maturity_slots: 1000,
        matured_haircut_bps: 300,
        max_accounts: 4096,
        min_nonzero_mm_req: 100,
        min_nonzero_im_req: 200,
    };

    let args = InitializeMarketArgs {
        init_slot: slot,
        init_oracle_price: 1000, // $10.00 default oracle price
        params,
    };

    let ix = ix::initialize_market_ix(&config.program_id, &market_pda, &config.authority(), args);

    println!("Deploying market...");
    println!("  Authority: {}", config.authority());
    println!("  Market PDA: {market_pda}");
    println!("  RPC: {}", config.rpc_url);

    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    println!("Market deployed.");

    Ok(())
}
