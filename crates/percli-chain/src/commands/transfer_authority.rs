use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, TransferAuthorityArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, new_authority: &Pubkey) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::transfer_authority_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        TransferAuthorityArgs {
            new_authority: *new_authority,
        },
    );

    if *new_authority == Pubkey::default() {
        println!("Cancelling any in-flight authority transfer (pending_authority -> default)...");
    } else {
        println!("Initiating authority transfer to {new_authority}...");
        println!("  The new authority must run `percli chain accept-authority` to complete the transfer.");
    }
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
