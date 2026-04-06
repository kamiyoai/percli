use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, CrankArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, oracle: &Pubkey, funding_rate: i64) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::crank_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        oracle,
        CrankArgs { funding_rate },
    );

    println!("Cranking: oracle={oracle}, funding_rate={funding_rate}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
