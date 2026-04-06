use anchor_lang::prelude::*;

use crate::error::{from_risk_error, PercolatorError};
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Trade<'info> {
    /// Matcher authority — must match the matcher stored in the market header.
    pub signer: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() == MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(
    ctx: Context<Trade>,
    account_a: u16,
    account_b: u16,
    size_q: i128,
    exec_price: u64,
    funding_rate: i64,
) -> Result<()> {
    require!(account_a != account_b, PercolatorError::InvalidMatchingEngine);

    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(&data[0..8] == b"percmrkt", PercolatorError::AccountNotFound);

    // Verify signer is the designated matcher authority
    let header = header_from_account_data(&data)?;
    require!(
        header.matcher == ctx.accounts.signer.key(),
        PercolatorError::Unauthorized
    );

    let engine = engine_from_account_data(&mut data);
    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    engine
        .execute_trade_not_atomic(
            account_a,
            account_b,
            oracle_price,
            clock.slot,
            size_q,
            exec_price,
            funding_rate,
        )
        .map_err(from_risk_error)?;

    Ok(())
}
