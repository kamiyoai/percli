use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix;
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, new_oracle: &Pubkey) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::update_oracle_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        new_oracle,
    );

    println!("Updating oracle to {new_oracle}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
