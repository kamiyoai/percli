use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, WithdrawInsuranceArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    amount: u64,
    mint: &Pubkey,
    authority_token_account: &Pubkey,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let (vault, _) = config.vault_pda();

    let ix = ix::withdraw_insurance_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        mint,
        authority_token_account,
        &vault,
        &spl_token::id(),
        WithdrawInsuranceArgs { amount },
    );

    println!("Withdrawing {amount} from insurance fund...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
