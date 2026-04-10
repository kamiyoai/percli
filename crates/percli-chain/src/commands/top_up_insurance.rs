use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::config::ChainConfig;
use crate::ix::{self, TopUpInsuranceArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    amount: u64,
    mint: &Pubkey,
    depositor_token_account: &Pubkey,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let (vault, _) = config.vault_pda();

    let ix = ix::top_up_insurance_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        mint,
        depositor_token_account,
        &vault,
        &spl_token::id(),
        TopUpInsuranceArgs { amount },
    );

    println!("Topping up insurance fund with {amount}...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
