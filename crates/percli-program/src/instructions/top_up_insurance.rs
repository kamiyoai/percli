use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct TopUpInsurance<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    pub mint: Account<'info, Mint>,

    /// Depositor's token account to transfer from.
    #[account(
        mut,
        constraint = depositor_token_account.owner == depositor.key(),
        constraint = depositor_token_account.mint == mint.key(),
    )]
    pub depositor_token_account: Account<'info, TokenAccount>,

    /// Vault token account to transfer into.
    #[account(
        mut,
        seeds = [b"vault", market.key().as_ref()],
        bump,
        constraint = vault.mint == mint.key(),
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<TopUpInsurance>, amount: u64) -> Result<()> {
    require!(amount > 0, PercolatorError::InsufficientBalance);

    // Transfer tokens from depositor to vault
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.depositor_token_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        &data[0..8] == b"percmrkt",
        PercolatorError::AccountNotFound
    );

    let header = header_from_account_data(&data)?;
    require!(
        header.mint == ctx.accounts.mint.key(),
        PercolatorError::Unauthorized
    );

    let engine = engine_from_account_data(&mut data);
    let clock = Clock::get()?;

    engine
        .top_up_insurance_fund(amount as u128, clock.slot)
        .map_err(from_risk_error)?;

    emit!(events::InsuranceToppedUp {
        depositor: ctx.accounts.depositor.key(),
        amount,
    });

    Ok(())
}
