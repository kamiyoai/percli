use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, DepositFeeCreditsArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    account_idx: u16,
    amount: u64,
    mint: &Pubkey,
    user_token_account: &Pubkey,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let (vault, _) = config.vault_pda();

    let ix = ix::deposit_fee_credits_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        mint,
        user_token_account,
        &vault,
        &spl_token::id(),
        DepositFeeCreditsArgs { account_idx, amount },
    );

    println!("Depositing {amount} fee credits for account {account_idx}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
