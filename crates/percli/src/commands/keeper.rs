use anyhow::Result;
use percli_chain::rpc::ChainRpc;
use percli_chain::ChainConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::Duration;

pub fn run(config: &ChainConfig, interval_secs: u64, pyth_feed: &str) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let oracle = Pubkey::from_str(pyth_feed)
        .map_err(|_| anyhow::anyhow!("Invalid Pyth feed pubkey: {pyth_feed}"))?;

    println!("Keeper started");
    println!("  Market:   {market_pda}");
    println!("  Oracle:   {oracle}");
    println!("  Interval: {interval_secs}s");
    println!();

    let interval = Duration::from_secs(interval_secs);
    let mut tick = 0u64;

    loop {
        tick += 1;

        let data = match rpc.get_account_data(&market_pda) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[tick {tick}] Failed to read market: {e}");
                std::thread::sleep(interval);
                continue;
            }
        };

        let engine = match percli_chain::read::engine_from_data(&data) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[tick {tick}] Failed to parse market: {e}");
                std::thread::sleep(interval);
                continue;
            }
        };

        // Crank every tick — the on-chain program reads the price directly from Pyth
        println!(
            "[tick {tick}] Cranking (last oracle price: {})",
            engine.last_oracle_price
        );
        if let Err(e) = percli_chain::commands::crank::run(config, &oracle, 0) {
            eprintln!("[tick {tick}] Crank failed: {e}");
        }

        // Check each account for liquidation
        for i in 0..engine.accounts.len() {
            if !engine.is_used(i) {
                continue;
            }
            let acct = &engine.accounts[i];
            let eff_pos = engine.effective_pos_q(i);
            if eff_pos == 0 {
                continue;
            }
            let above_mm = engine.is_above_maintenance_margin(acct, i, engine.last_oracle_price);
            if !above_mm {
                println!("[tick {tick}] Liquidating account {i} (below maintenance margin)");
                if let Err(e) = percli_chain::commands::liquidate::run(config, i as u16, 0) {
                    eprintln!("[tick {tick}] Liquidation failed for account {i}: {e}");
                }
            }
        }

        std::thread::sleep(interval);
    }
}
