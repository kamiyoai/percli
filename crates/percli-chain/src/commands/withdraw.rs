use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, WithdrawArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    account_idx: u16,
    amount: u64,
    funding_rate: i64,
    mint: &Pubkey,
    user_token_account: &Pubkey,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let (vault, _) = config.vault_pda();

    let ix = ix::withdraw_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        mint,
        user_token_account,
        &vault,
        &spl_token::id(),
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
