use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{engine_from_account_data, header_from_account_data, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,

    /// The collateral mint for this market.
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

pub fn handler(ctx: Context<Deposit>, account_idx: u16, amount: u64) -> Result<()> {
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

    // Update engine state
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let header = header_from_account_data(&data)?;
    require!(header.mint == ctx.accounts.mint.key(), PercolatorError::Unauthorized);

    let engine = engine_from_account_data(&mut data);

    // If account already exists, verify the signer is the owner
    let user_key = ctx.accounts.user.key();
    if engine.is_used(account_idx as usize) {
        let existing_owner = engine.accounts[account_idx as usize].owner;
        if existing_owner != [0u8; 32] {
            require!(existing_owner == user_key.to_bytes(), PercolatorError::Unauthorized);
        }
    }

    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    engine
        .deposit(account_idx, amount as u128, oracle_price, clock.slot)
        .map_err(from_risk_error)?;

    // If this was a new account (owner is zero), claim it for the depositor
    let _ = engine.set_owner(account_idx, user_key.to_bytes());

    emit!(events::Deposited {
        user: ctx.accounts.user.key(),
        account_idx,
        amount,
    });

    Ok(())
}
