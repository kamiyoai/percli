use anchor_lang::prelude::*;
use pyth_sdk_solana::state::{load_price_account, PriceStatus};

use crate::error::{from_risk_error, PercolatorError};
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

/// Maximum age of a Pyth price update before it's considered stale (seconds).
const MAX_PRICE_AGE_SECS: i64 = 60;

/// Pyth v2 oracle program on mainnet/devnet.
const PYTH_PROGRAM_ID: Pubkey = pubkey!("FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH");

#[derive(Accounts)]
pub struct Crank<'info> {
    /// Permissionless — anyone can crank.
    pub cranker: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    /// CHECK: Pyth price feed account. Validated by owner check and pyth_sdk_solana deserialization.
    #[account(
        owner = PYTH_PROGRAM_ID @ PercolatorError::InvalidOraclePrice,
    )]
    pub oracle: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Crank>, funding_rate: i64) -> Result<()> {
    // Verify the oracle account matches the one stored in the market header
    {
        let data = ctx.accounts.market.try_borrow_data()?;
        require!(&data[0..8] == b"percmrkt", PercolatorError::AccountNotFound);
        let header = header_from_account_data(&data)?;
        require!(
            header.oracle == ctx.accounts.oracle.key(),
            PercolatorError::InvalidOraclePrice
        );
    }

    // Read oracle price from Pyth — deserialize directly from raw bytes
    // to avoid solana-pubkey version mismatch between pyth-sdk-solana and anchor-lang 1.0.
    let oracle_data = ctx.accounts.oracle.data.borrow();
    let price_account = load_price_account::<32, ()>(&oracle_data)
        .map_err(|_| error!(PercolatorError::InvalidOraclePrice))?;

    // Require Trading status
    require!(
        price_account.agg.status == PriceStatus::Trading,
        PercolatorError::InvalidOraclePrice
    );

    // Check staleness — reject future timestamps (would bypass the age check via saturating_sub)
    let clock = Clock::get()?;
    let current_timestamp = clock.unix_timestamp;
    let price_age = current_timestamp
        .checked_sub(price_account.timestamp)
        .ok_or_else(|| error!(PercolatorError::StaleOracle))?;
    require!(price_age <= MAX_PRICE_AGE_SECS, PercolatorError::StaleOracle);

    let price = price_account.agg.price;
    let expo = price_account.expo;

    // Price must be positive
    require!(price > 0, PercolatorError::InvalidOraclePriceValue);

    // Bound exponent to reasonable range to prevent overflow/truncation-to-zero
    require!(expo >= -18 && expo <= 18, PercolatorError::InvalidOraclePrice);

    // Convert Pyth price to u64 oracle price for the engine.
    // Pyth prices have an exponent (e.g. price=12345, expo=-2 means $123.45).
    // The engine uses raw integer prices, so normalize to a consistent scale.
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

    // Drop the oracle borrow before mutably borrowing market
    drop(oracle_data);

    // Update engine
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        &data[0..8] == b"percmrkt",
        PercolatorError::AccountNotFound
    );

    let engine = engine_from_account_data(&mut data);

    engine
        .keeper_crank_not_atomic(clock.slot, oracle_price, &[], 0, funding_rate)
        .map_err(from_risk_error)?;

    Ok(())
}
