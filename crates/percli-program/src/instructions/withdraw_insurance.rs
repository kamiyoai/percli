use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{
    engine_from_account_data, header_from_account_data, market_signer_seeds, MARKET_ACCOUNT_SIZE,
};

#[derive(Accounts)]
pub struct WithdrawInsurance<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    pub mint: Account<'info, Mint>,

    /// Authority's token account to receive withdrawn insurance tokens.
    #[account(
        mut,
        constraint = authority_token_account.owner == authority.key(),
        constraint = authority_token_account.mint == mint.key(),
    )]
    pub authority_token_account: Account<'info, TokenAccount>,

    /// Vault token account to transfer from.
    #[account(
        mut,
        seeds = [b"vault", market.key().as_ref()],
        bump,
        constraint = vault.mint == mint.key(),
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<WithdrawInsurance>, amount: u64) -> Result<()> {
    require!(amount > 0, PercolatorError::InsufficientBalance);

    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        &data[0..8] == b"percmrkt",
        PercolatorError::AccountNotFound
    );

    let header = header_from_account_data(&data)?;
    require!(
        header.authority == ctx.accounts.authority.key(),
        PercolatorError::Unauthorized
    );
    require!(
        header.mint == ctx.accounts.mint.key(),
        PercolatorError::Unauthorized
    );

    let engine = engine_from_account_data(&mut data);
    let clock = Clock::get()?;

    engine
        .withdraw_insurance(amount as u128, clock.slot)
        .map_err(from_risk_error)?;

    drop(data);

    let bump = [header.bump];
    let signer_seeds = market_signer_seeds(&header.authority, &bump);
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.authority_token_account.to_account_info(),
                authority: ctx.accounts.market.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
            &[&signer_seeds],
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(events::InsuranceWithdrawn {
        authority: ctx.accounts.authority.key(),
        amount,
    });

    Ok(())
}
