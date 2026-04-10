use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, UpdateMatcherArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, new_matcher: &Pubkey) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::update_matcher_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        UpdateMatcherArgs {
            new_matcher: *new_matcher,
        },
    );

    println!("Updating matcher to {new_matcher}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
