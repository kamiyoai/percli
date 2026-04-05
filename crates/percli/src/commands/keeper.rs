use anyhow::Result;
use percli_chain::rpc::ChainRpc;
use percli_chain::ChainConfig;
use std::time::Duration;

pub fn run(config: &ChainConfig, interval_secs: u64, pyth_feed: Option<&str>) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();

    println!("Keeper started");
    println!("  Market:   {market_pda}");
    println!("  Interval: {interval_secs}s");
    if let Some(feed) = pyth_feed {
        println!("  Pyth:     {feed}");
    }
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

        // Read oracle price from Pyth if configured, otherwise use on-chain price
        let oracle_price = if let Some(feed_id) = pyth_feed {
            match read_pyth_price(&rpc, feed_id) {
                Some(price) => price,
                None => {
                    eprintln!("[tick {tick}] Pyth price unavailable, using on-chain oracle");
                    engine.last_oracle_price
                }
            }
        } else {
            engine.last_oracle_price
        };

        // Crank if oracle price changed
        if oracle_price != engine.last_oracle_price {
            println!(
                "[tick {tick}] Cranking: oracle {oracle_price} (was {})",
                engine.last_oracle_price
            );
            if let Err(e) = percli_chain::commands::crank::run(config, oracle_price, 0) {
                eprintln!("[tick {tick}] Crank failed: {e}");
            }
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
            let above_mm = engine.is_above_maintenance_margin(acct, i, oracle_price);
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

fn read_pyth_price(rpc: &ChainRpc, feed_id: &str) -> Option<u64> {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    let pubkey = Pubkey::from_str(feed_id).ok()?;
    let data = rpc.get_account_data(&pubkey).ok()?;

    // Parse Pyth price account: price at offset 208, expo at offset 20
    // This is a simplified reader for the Pyth v2 price account format.
    // For production, use pyth-sdk-solana behind the `pyth` feature.
    if data.len() < 216 {
        return None;
    }
    let expo = i32::from_le_bytes(data[20..24].try_into().ok()?);
    let price = i64::from_le_bytes(data[208..216].try_into().ok()?);

    if price <= 0 {
        return None;
    }

    let oracle = if expo >= 0 {
        (price as u64).saturating_mul(10u64.saturating_pow(expo as u32))
    } else {
        (price as u64) / 10u64.saturating_pow((-expo) as u32)
    };

    Some(oracle)
}
