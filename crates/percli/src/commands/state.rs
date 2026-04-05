use anyhow::{Context, Result};
use percli_core::{NamedEngine, ParamsConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

const STATE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Deposit {
        account: String,
        amount: u64,
    },
    Withdraw {
        account: String,
        amount: u64,
    },
    Trade {
        long: String,
        short: String,
        size_q: i64,
        price: u64,
    },
    Crank {
        oracle: u64,
        slot: u64,
    },
    SetOracle {
        price: u64,
    },
    SetSlot {
        slot: u64,
    },
    SetFundingRate {
        rate: i64,
    },
    Liquidate {
        account: String,
    },
    Settle {
        account: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub version: u32,
    pub params: ParamsConfig,
    pub oracle_price: u64,
    pub slot: u64,
    pub funding_rate: i64,
    pub operations: Vec<Operation>,
}

impl SavedState {
    pub fn new(params: ParamsConfig, oracle_price: u64, slot: u64) -> Self {
        Self {
            version: STATE_VERSION,
            params,
            oracle_price,
            slot,
            funding_rate: 0,
            operations: Vec::new(),
        }
    }
}

pub fn load_or_create(path: &Path) -> Result<SavedState> {
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read state file: {}", path.display()))?;
        let state: SavedState = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse state file: {}", path.display()))?;
        if state.version < STATE_VERSION {
            anyhow::bail!(
                "state file version {} is too old (expected {}). \
                 delete the file and recreate it",
                state.version,
                STATE_VERSION,
            );
        }
        Ok(state)
    } else {
        Ok(SavedState::new(ParamsConfig::default(), 1000, 0))
    }
}

pub fn save(path: &Path, state: &SavedState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)
        .with_context(|| format!("failed to write temp state file: {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename state file: {}", path.display()))?;
    Ok(())
}

pub fn replay(state: &SavedState) -> Result<NamedEngine> {
    let params = state.params.to_risk_params();
    let mut engine = NamedEngine::new(params, state.slot, state.oracle_price);
    engine.set_funding_rate(state.funding_rate);

    for (i, op) in state.operations.iter().enumerate() {
        replay_op(&mut engine, op)
            .with_context(|| format!("failed replaying operation {}", i + 1))?;
    }

    Ok(engine)
}

fn replay_op(engine: &mut NamedEngine, op: &Operation) -> Result<()> {
    match op {
        Operation::Deposit { account, amount } => {
            engine.deposit(account, *amount as u128)?;
        }
        Operation::Withdraw { account, amount } => {
            engine.withdraw(account, *amount as u128)?;
        }
        Operation::Trade {
            long,
            short,
            size_q,
            price,
        } => {
            engine.trade(long, short, *size_q as i128, *price)?;
        }
        Operation::Crank { oracle, slot } => {
            engine.crank(*oracle, *slot)?;
        }
        Operation::SetOracle { price } => {
            engine.set_oracle(*price)?;
        }
        Operation::SetSlot { slot } => {
            engine.set_slot(*slot);
        }
        Operation::SetFundingRate { rate } => {
            engine.set_funding_rate(*rate);
        }
        Operation::Liquidate { account } => {
            engine.liquidate(account)?;
        }
        Operation::Settle { account } => {
            engine.settle(account)?;
        }
    }
    Ok(())
}
