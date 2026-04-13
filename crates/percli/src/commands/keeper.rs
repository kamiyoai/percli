use anyhow::Result;
use percli_chain::rpc::ChainRpc;
use percli_chain::ChainConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

struct KeeperStats {
    start_time: Instant,
    total_cranks: u64,
    failed_cranks: u64,
    total_liquidations: u64,
    failed_liquidations: u64,
    total_reclaims: u64,
}

impl KeeperStats {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_cranks: 0,
            failed_cranks: 0,
            total_liquidations: 0,
            failed_liquidations: 0,
            total_reclaims: 0,
        }
    }

    fn log_summary(&self) {
        let uptime = self.start_time.elapsed().as_secs();
        info!(
            uptime_s = uptime,
            total_cranks = self.total_cranks,
            failed_cranks = self.failed_cranks,
            total_liquidations = self.total_liquidations,
            failed_liquidations = self.failed_liquidations,
            total_reclaims = self.total_reclaims,
            "keeper summary"
        );
    }
}

pub fn run(config: &ChainConfig, interval_secs: u64, pyth_feed: &str) -> Result<()> {
    let rpc = ChainRpc::new(&config.rpc_url);
    let (market_pda, _) = config.market_pda();
    let oracle = Pubkey::from_str(pyth_feed)
        .map_err(|_| anyhow::anyhow!("Invalid Pyth feed pubkey: {pyth_feed}"))?;

    info!(
        market = %market_pda,
        oracle = %oracle,
        interval_s = interval_secs,
        "keeper started"
    );

    let interval = Duration::from_secs(interval_secs);
    let mut tick = 0u64;
    let mut stats = KeeperStats::new();

    loop {
        tick += 1;

        let data = match rpc.get_account_data(&market_pda) {
            Ok(d) => d,
            Err(e) => {
                error!(tick, error = %e, "failed to read market");
                std::thread::sleep(interval);
                continue;
            }
        };

        let engine = match percli_chain::read::engine_from_data(&data) {
            Ok(e) => e,
            Err(e) => {
                error!(tick, error = %e, "failed to parse market");
                std::thread::sleep(interval);
                continue;
            }
        };

        // Crank every tick
        let crank_start = Instant::now();
        info!(
            tick,
            last_oracle_price = engine.last_oracle_price,
            "cranking"
        );
        match percli_chain::commands::crank::run(config, &oracle, 0) {
            Ok(()) => {
                stats.total_cranks += 1;
                debug!(
                    tick,
                    elapsed_ms = crank_start.elapsed().as_millis() as u64,
                    "crank success"
                );
            }
            Err(e) => {
                stats.failed_cranks += 1;
                warn!(tick, error = %e, "crank failed");
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
            let above_mm = engine.is_above_maintenance_margin(acct, i, engine.last_oracle_price);
            if !above_mm {
                info!(
                    tick,
                    account_idx = i,
                    "liquidating (below maintenance margin)"
                );
                match percli_chain::commands::liquidate::run(config, i as u16, 0) {
                    Ok(()) => {
                        stats.total_liquidations += 1;
                    }
                    Err(e) => {
                        stats.failed_liquidations += 1;
                        warn!(tick, account_idx = i, error = %e, "liquidation failed");
                    }
                }
            }
        }

        // Reclaim dead accounts (flat with dust capital)
        for i in 0..engine.accounts.len() {
            if !engine.is_used(i) {
                continue;
            }
            let eff_pos = engine.effective_pos_q(i);
            if eff_pos != 0 {
                continue;
            }
            let capital = engine.accounts[i].capital.get();
            let min_deposit = engine.params.min_initial_deposit.get();
            if capital > 0 && capital < min_deposit {
                debug!(tick, account_idx = i, capital, "reclaiming dust account");
                match percli_chain::commands::reclaim::run(config, i as u16) {
                    Ok(()) => {
                        stats.total_reclaims += 1;
                    }
                    Err(e) => {
                        debug!(tick, account_idx = i, error = %e, "reclaim failed");
                    }
                }
            }
        }

        // Log summary periodically
        if tick % 10 == 0 {
            stats.log_summary();
        }

        std::thread::sleep(interval);
    }
}
