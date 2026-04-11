use anyhow::Result;

use crate::config::ChainConfig;
use crate::ix;
use crate::rpc::ChainRpc;

pub fn run(config: &ChainConfig) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    let ix = ix::migrate_header_v1_ix(&config.program_id, &market_pda, &config.authority());

    println!("Migrating market header from v0 (136 bytes) to v1 (168 bytes)...");
    let sig = rpc.send_tx(&[ix], &config.keypair)?;
    println!("  Tx: {sig}");
    println!("  Engine bytes shifted forward by 32; pending_authority slot added.");
    println!("  Discriminator version byte stamped to 0x01 (v1).");
    Ok(())
}
