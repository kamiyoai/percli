use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix;
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, oracle: &Pubkey) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::accrue_market_ix(&config.program_id, &market_pda, &config.authority(), oracle);

    println!("Accruing market state...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
