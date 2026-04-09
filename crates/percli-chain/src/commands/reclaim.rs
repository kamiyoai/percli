use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, ReclaimAccountArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, account_idx: u16) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::reclaim_account_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        ReclaimAccountArgs { account_idx },
    );

    println!("Reclaiming account slot {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
