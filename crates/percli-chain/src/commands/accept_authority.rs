use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix;
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::accept_authority_ix(&config.program_id, &market_pda, &config.authority());

    println!("Accepting authority for market {market_pda}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    println!("  You are now the market authority.");
    Ok(())
}
