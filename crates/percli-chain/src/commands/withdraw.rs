use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, WithdrawArgs};
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig, account_idx: u16, amount: u128, funding_rate: i64) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::withdraw_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        WithdrawArgs {
            account_idx,
            amount,
            funding_rate,
        },
    );

    println!("Withdrawing {amount} from account slot {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
