use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, CrankArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, oracle_price: u64, funding_rate: i64) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::crank_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        CrankArgs {
            oracle_price,
            funding_rate,
        },
    );

    println!("Cranking: oracle_price={oracle_price}, funding_rate={funding_rate}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
