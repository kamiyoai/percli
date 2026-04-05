use anchor_lang::prelude::*;

use crate::error::{from_risk_error, PercolatorError};
use crate::state::{engine_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated via discriminator, PDA seeds, and size.
    #[account(
        mut,
        constraint = market.data_len() == MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<Withdraw>,
    account_idx: u16,
    amount: u128,
    funding_rate: i64,
) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(&data[0..8] == b"percmrkt", PercolatorError::AccountNotFound);

    let engine = engine_from_account_data(&mut data);
    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    engine
        .withdraw_not_atomic(account_idx, amount, oracle_price, clock.slot, funding_rate)
        .map_err(from_risk_error)?;

    // TODO: integrate SPL token transfer from vault to user

    Ok(())
}
