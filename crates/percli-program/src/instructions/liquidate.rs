use anchor_lang::prelude::*;
use percli_core::LiquidationPolicy;

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Liquidate<'info> {
    /// Permissionless — anyone can liquidate.
    pub liquidator: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Liquidate>, account_idx: u16, funding_rate: i64) -> Result<bool> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(crate::state::is_v1_market(&data), PercolatorError::AccountNotFound);

    let engine = engine_from_account_data(&mut data);
    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    let liquidated = engine
        .liquidate_at_oracle_not_atomic(
            account_idx,
            clock.slot,
            oracle_price,
            LiquidationPolicy::FullClose,
            funding_rate,
        )
        .map_err(from_risk_error)?;

    emit!(events::Liquidated {
        liquidator: ctx.accounts.liquidator.key(),
        account_idx,
        liquidated,
    });

    Ok(liquidated)
}
