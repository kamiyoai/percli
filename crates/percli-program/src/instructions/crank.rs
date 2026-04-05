use anchor_lang::prelude::*;

use crate::error::{from_risk_error, PercolatorError};
use crate::state::{engine_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Crank<'info> {
    /// Permissionless — anyone can crank.
    pub cranker: Signer<'info>,

    /// CHECK: Validated via discriminator and size.
    #[account(
        mut,
        constraint = market.data_len() == MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
    // TODO: add Pyth oracle account for production
    // pub oracle: Account<'info, PriceUpdateV2>,
}

pub fn handler(ctx: Context<Crank>, oracle_price: u64, funding_rate: i64) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(&data[0..8] == b"percmrkt", PercolatorError::AccountNotFound);

    let engine = engine_from_account_data(&mut data);
    let clock = Clock::get()?;

    // In production, oracle_price would be read from Pyth price feed.
    engine
        .keeper_crank_not_atomic(clock.slot, oracle_price, &[], 0, funding_rate)
        .map_err(from_risk_error)?;

    Ok(())
}
