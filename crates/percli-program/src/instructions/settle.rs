use anchor_lang::prelude::*;

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Settle<'info> {
    pub user: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Settle>, account_idx: u16, funding_rate: i64) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let engine = engine_from_account_data(&mut data);

    // Verify signer owns this account
    let account_owner = engine.accounts[account_idx as usize].owner;
    require!(
        account_owner == ctx.accounts.user.key().to_bytes(),
        PercolatorError::Unauthorized
    );

    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    engine
        .settle_account_not_atomic(account_idx, oracle_price, clock.slot, funding_rate)
        .map_err(from_risk_error)?;

    emit!(events::Settled {
        user: ctx.accounts.user.key(),
        account_idx,
    });

    Ok(())
}
