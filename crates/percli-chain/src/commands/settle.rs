use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, SettleArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, account_idx: u16, funding_rate: i64) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::settle_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        SettleArgs {
            account_idx,
            funding_rate,
        },
    );

    println!("Settling account slot {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
