use anchor_lang::prelude::*;

use crate::error::PercolatorError;
use crate::instructions::events;
use crate::state::{header_from_account_data, write_header, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct UpdateMatcher<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<UpdateMatcher>, new_matcher: Pubkey) -> Result<()> {
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

    let old_matcher = header.matcher;
    header.matcher = new_matcher;
    write_header(&mut data, &header);

    emit!(events::MatcherUpdated {
        authority: ctx.accounts.authority.key(),
        old_matcher,
        new_matcher,
    });

    Ok(())
}
