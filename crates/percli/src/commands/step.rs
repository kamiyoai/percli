use anyhow::Result;
use percli_core::POS_SCALE;
use std::path::Path;

use super::state::{self, Operation};
use crate::format::{self, status, OutputFormat};
use crate::StepAction;

pub fn run(state_path: &str, action: StepAction, fmt: OutputFormat) -> Result<()> {
    let path = Path::new(state_path);
    let mut state = state::load_or_create(path)?;

    let op = action_to_op(&action)?;
    let desc = describe_action(&action);
    status::status("Stepping", &desc);

    state.operations.push(op);

    match &action {
        StepAction::Crank { oracle, slot } => {
            state.oracle_price = *oracle;
            state.slot = *slot;
        }
        StepAction::SetOracle { price } => {
            state.oracle_price = *price;
        }
        StepAction::SetSlot { slot } => {
            state.slot = *slot;
        }
        StepAction::SetFundingRate { rate } => {
            state.funding_rate = *rate;
        }
        _ => {}
    }

    let engine = state::replay(&state)?;
    state::save(path, &state)?;

    let snap = engine.snapshot();
    format::print_snapshot(&snap, fmt)?;

    Ok(())
}

fn describe_action(action: &StepAction) -> String {
    match action {
        StepAction::Deposit { account, amount } => format!("deposit {account} {amount}"),
        StepAction::Withdraw { account, amount } => format!("withdraw {account} {amount}"),
        StepAction::Trade {
            long,
            short,
            size,
            price,
        } => format!("trade {long}/{short} size={size} price={price}"),
        StepAction::Crank { oracle, slot } => format!("crank oracle={oracle} slot={slot}"),
        StepAction::Liquidate { account } => format!("liquidate {account}"),
        StepAction::Settle { account } => format!("settle {account}"),
        StepAction::SetOracle { price } => format!("set-oracle {price}"),
        StepAction::SetSlot { slot } => format!("set-slot {slot}"),
        StepAction::SetFundingRate { rate } => format!("set-funding-rate {rate}"),
    }
}

fn checked_u128_to_u64(val: u128, field: &str) -> Result<u64> {
    u64::try_from(val).map_err(|_| anyhow::anyhow!("{field} {val} exceeds maximum ({})", u64::MAX))
}

fn checked_i128_to_i64(val: i128, field: &str) -> Result<i64> {
    i64::try_from(val).map_err(|_| anyhow::anyhow!("{field} {val} exceeds range"))
}

fn action_to_op(action: &StepAction) -> Result<Operation> {
    Ok(match action {
        StepAction::Deposit { account, amount } => Operation::Deposit {
            account: account.clone(),
            amount: checked_u128_to_u64(*amount, "amount")?,
        },
        StepAction::Withdraw { account, amount } => Operation::Withdraw {
            account: account.clone(),
            amount: checked_u128_to_u64(*amount, "amount")?,
        },
        StepAction::Trade {
            long,
            short,
            size,
            price,
        } => {
            let size_q = size
                .checked_mul(POS_SCALE as i128)
                .ok_or_else(|| anyhow::anyhow!("size {size} overflows when scaled by POS_SCALE"))?;
            Operation::Trade {
                long: long.clone(),
                short: short.clone(),
                size_q: checked_i128_to_i64(size_q, "size_q")?,
                price: *price,
            }
        }
        StepAction::Crank { oracle, slot } => Operation::Crank {
            oracle: *oracle,
            slot: *slot,
        },
        StepAction::Liquidate { account } => Operation::Liquidate {
            account: account.clone(),
        },
        StepAction::Settle { account } => Operation::Settle {
            account: account.clone(),
        },
        StepAction::SetOracle { price } => Operation::SetOracle { price: *price },
        StepAction::SetSlot { slot } => Operation::SetSlot { slot: *slot },
        StepAction::SetFundingRate { rate } => Operation::SetFundingRate { rate: *rate },
    })
}
