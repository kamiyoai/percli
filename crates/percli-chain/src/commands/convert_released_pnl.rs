use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, ConvertReleasedPnlArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    account_idx: u16,
    x_req: u64,
    oracle: &Pubkey,
    funding_rate: i64,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::convert_released_pnl_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        oracle,
        ConvertReleasedPnlArgs {
            account_idx,
            x_req,
            funding_rate,
        },
    );

    println!("Converting {x_req} released PnL for account {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
