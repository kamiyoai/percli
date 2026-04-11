use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct DepositFeeCredits<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    pub mint: Account<'info, Mint>,

    /// User's token account to transfer from.
    #[account(
        mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

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

pub fn handler(ctx: Context<DepositFeeCredits>, account_idx: u16, amount: u64) -> Result<()> {
    require!(amount > 0, PercolatorError::InsufficientBalance);

    // Transfer tokens from user to vault
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let header = header_from_account_data(&data)?;
    require!(
        header.mint == ctx.accounts.mint.key(),
        PercolatorError::Unauthorized
    );

    let engine = engine_from_account_data(&mut data);

    // Verify account ownership
    require!(
        engine.is_used(account_idx as usize),
        PercolatorError::AccountNotFound
    );
    require!(
        engine.accounts[account_idx as usize].owner == ctx.accounts.user.key().to_bytes(),
        PercolatorError::Unauthorized
    );

    let clock = Clock::get()?;

    engine
        .deposit_fee_credits(account_idx, amount as u128, clock.slot)
        .map_err(from_risk_error)?;

    emit!(events::FeeCreditsDeposited {
        user: ctx.accounts.user.key(),
        account_idx,
        amount,
    });

    Ok(())
}
