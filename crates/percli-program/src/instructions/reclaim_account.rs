use anchor_lang::prelude::*;

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct ReclaimAccount<'info> {
    /// Permissionless — anyone can reclaim dead accounts.
    pub reclaimer: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<ReclaimAccount>, account_idx: u16) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let engine = engine_from_account_data(&mut data);
    let clock = Clock::get()?;

    engine
        .reclaim_empty_account_not_atomic(account_idx, clock.slot)
        .map_err(from_risk_error)?;

    emit!(events::AccountReclaimed {
        reclaimer: ctx.accounts.reclaimer.key(),
        account_idx,
    });

    Ok(())
}
