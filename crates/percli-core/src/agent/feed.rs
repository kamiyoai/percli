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
    Inline { prices: Vec<PriceTick> },
    Csv { path: PathBuf },
    Stdin,
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
        }
    }
}
