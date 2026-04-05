use serde::Deserialize;
use std::io::BufRead;
use std::path::PathBuf;

/// A single price observation in the feed.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceTick {
    pub oracle: u64,
    pub slot: u64,
}

/// Source of price data for the agent tick loop.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeedConfig {
    Inline {
        prices: Vec<PriceTick>,
    },
    Csv {
        path: PathBuf,
    },
    Stdin,
    Pyth {
        rpc_url: String,
        feed_id: String,
        #[serde(default = "default_poll_ms")]
        poll_ms: u64,
        #[serde(default = "default_max_ticks")]
        max_ticks: u64,
    },
}

fn default_poll_ms() -> u64 {
    2000
}

fn default_max_ticks() -> u64 {
    1000
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self::Inline {
            prices: vec![PriceTick {
                oracle: 1000,
                slot: 100,
            }],
        }
    }
}

impl FeedConfig {
    /// Convert the feed config into a boxed iterator of price ticks.
    ///
    /// For CSV and stdin feeds, parse errors are silently skipped.
    pub fn into_tick_iter(self) -> anyhow::Result<Box<dyn Iterator<Item = PriceTick>>> {
        match self {
            FeedConfig::Inline { prices } => Ok(Box::new(prices.into_iter())),
            FeedConfig::Csv { path } => {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    anyhow::anyhow!("failed to read feed CSV {}: {}", path.display(), e)
                })?;
                let ticks: Vec<PriceTick> = content
                    .lines()
                    .filter(|line| !line.starts_with('#') && !line.is_empty())
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                        if parts.len() >= 2 {
                            let oracle = parts[0].parse().ok()?;
                            let slot = parts[1].parse().ok()?;
                            Some(PriceTick { oracle, slot })
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(Box::new(ticks.into_iter()))
            }
            FeedConfig::Stdin => {
                let reader = std::io::stdin().lock();
                let ticks: Vec<PriceTick> = reader
                    .lines()
                    .map_while(Result::ok)
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| serde_json::from_str(&line).ok())
                    .collect();
                Ok(Box::new(ticks.into_iter()))
            }
            #[cfg(feature = "pyth")]
            FeedConfig::Pyth {
                rpc_url,
                feed_id,
                poll_ms,
                max_ticks,
            } => {
                use pyth_sdk_solana::load_price_feed_from_account_info;
                use solana_client::rpc_client::RpcClient;
                use solana_sdk::commitment_config::CommitmentConfig;

                let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
                let feed_pubkey: solana_sdk::pubkey::Pubkey = feed_id
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid Pyth feed pubkey: {}", feed_id))?;

                let mut ticks = Vec::new();
                let mut last_slot = 0u64;

                for _ in 0..max_ticks {
                    match client.get_account(&feed_pubkey) {
                        Ok(account) => {
                            let mut data = account.data.as_slice();
                            if let Ok(feed) = load_price_feed_from_account_info(&mut data) {
                                if let Some(price) = feed.get_price_no_older_than(60) {
                                    let slot = client.get_slot().unwrap_or(last_slot + 1);
                                    // Convert Pyth price (with exponent) to u64
                                    let oracle = if price.expo >= 0 {
                                        (price.price as u64)
                                            .saturating_mul(10u64.saturating_pow(price.expo as u32))
                                    } else {
                                        (price.price as u64)
                                            / 10u64.saturating_pow((-price.expo) as u32)
                                    };
                                    if slot > last_slot {
                                        ticks.push(PriceTick { oracle, slot });
                                        last_slot = slot;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("pyth feed error: {e}");
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(poll_ms));
                }
                Ok(Box::new(ticks.into_iter()))
            }
            #[cfg(not(feature = "pyth"))]
            FeedConfig::Pyth { .. } => {
                anyhow::bail!(
                    "Pyth feed requires the `pyth` feature. \
                     Install with: cargo install percli --features pyth"
                )
            }
        }
    }
}
