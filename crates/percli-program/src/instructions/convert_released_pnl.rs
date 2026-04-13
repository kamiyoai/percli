use anchor_lang::prelude::*;
use pyth_sdk_solana::state::{load_price_account, PriceStatus};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

const MAX_PRICE_AGE_SECS: i64 = 60;
const PYTH_PROGRAM_ID: Pubkey = pubkey!("FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH");

#[derive(Accounts)]
pub struct ConvertReleasedPnl<'info> {
    pub user: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    /// CHECK: Pyth price feed account.
    #[account(
        owner = PYTH_PROGRAM_ID @ PercolatorError::InvalidOraclePrice,
    )]
    pub oracle: UncheckedAccount<'info>,
}

pub fn handler(
    ctx: Context<ConvertReleasedPnl>,
    account_idx: u16,
    x_req: u64,
    funding_rate: i64,
) -> Result<()> {
    // Verify oracle matches header
    {
        let data = ctx.accounts.market.try_borrow_data()?;
        require!(
            crate::state::is_v1_market(&data),
            PercolatorError::AccountNotFound
        );
        let header = header_from_account_data(&data)?;
        require!(
            header.oracle == ctx.accounts.oracle.key(),
            PercolatorError::InvalidOraclePrice
        );
    }

    // Read Pyth oracle price
    let oracle_data = ctx.accounts.oracle.data.borrow();
    let price_account = load_price_account::<32, ()>(&oracle_data)
        .map_err(|_| error!(PercolatorError::InvalidOraclePrice))?;

    require!(
        price_account.agg.status == PriceStatus::Trading,
        PercolatorError::InvalidOraclePrice
    );

    let clock = Clock::get()?;
    let price_age = clock
        .unix_timestamp
        .checked_sub(price_account.timestamp)
        .ok_or_else(|| error!(PercolatorError::StaleOracle))?;
    require!(
        price_age <= MAX_PRICE_AGE_SECS,
        PercolatorError::StaleOracle
    );

    let price = price_account.agg.price;
    let expo = price_account.expo;

    require!(price > 0, PercolatorError::InvalidOraclePriceValue);
    require!(
        expo >= -18 && expo <= 18,
        PercolatorError::InvalidOraclePrice
    );

    let oracle_price = if expo >= 0 {
        (price as u64)
            .checked_mul(10u64.pow(expo as u32))
            .ok_or_else(|| error!(PercolatorError::InvalidOraclePriceValue))?
    } else {
        let divisor = 10u64.pow((-expo) as u32);
        (price as u64)
            .checked_div(divisor)
            .ok_or_else(|| error!(PercolatorError::InvalidOraclePriceValue))?
    };

    require!(oracle_price > 0, PercolatorError::InvalidOraclePriceValue);

    drop(oracle_data);

    // Update engine
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let engine = engine_from_account_data(&mut data);

    engine
        .convert_released_pnl_not_atomic(
            account_idx,
            x_req as u128,
            oracle_price,
            clock.slot,
            funding_rate,
        )
        .map_err(from_risk_error)?;

    emit!(events::PnlConverted {
        user: ctx.accounts.user.key(),
        account_idx,
        x_req,
    });

    Ok(())
}
