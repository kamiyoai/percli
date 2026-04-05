use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, DepositArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, account_idx: u16, amount: u128) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::deposit_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        DepositArgs {
            account_idx,
            amount,
        },
    );

    println!("Depositing {amount} to account slot {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
