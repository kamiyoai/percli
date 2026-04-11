use anchor_lang::prelude::*;

use crate::error::PercolatorError;
use crate::instructions::events;
use crate::state::{header_from_account_data, write_header, MARKET_ACCOUNT_SIZE};

const PYTH_PROGRAM_ID: Pubkey = pubkey!("FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH");

#[derive(Accounts)]
pub struct UpdateOracle<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    /// CHECK: Validated by Pyth program owner check.
    #[account(
        owner = PYTH_PROGRAM_ID @ PercolatorError::InvalidOraclePrice,
    )]
    pub new_oracle: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<UpdateOracle>) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let mut header = header_from_account_data(&data)?;
    require!(
        header.authority == ctx.accounts.authority.key(),
        PercolatorError::Unauthorized
    );

    let old_oracle = header.oracle;
    header.oracle = ctx.accounts.new_oracle.key();
    write_header(&mut data, &header);

    emit!(events::OracleUpdated {
        authority: ctx.accounts.authority.key(),
        old_oracle,
        new_oracle: ctx.accounts.new_oracle.key(),
    });

    Ok(())
}
