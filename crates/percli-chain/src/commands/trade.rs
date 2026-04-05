use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix::{self, TradeArgs};
use crate::rpc::ChainRpc;

pub fn run(
    config: &ChainConfig,
    account_a: u16,
    account_b: u16,
    size_q: i128,
    exec_price: u64,
    funding_rate: i64,
) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::trade_ix(
        &config.program_id,
        &market_pda,
        &config.authority(),
        TradeArgs {
            account_a,
            account_b,
            size_q,
            exec_price,
            funding_rate,
        },
    );

    println!(
        "Executing trade: account {account_a} <> {account_b}, size {size_q}, price {exec_price}..."
    );
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    Ok(())
}
