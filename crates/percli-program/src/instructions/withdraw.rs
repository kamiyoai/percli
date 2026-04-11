use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::error::{from_risk_error, PercolatorError};
use crate::instructions::events;
use crate::state::{
    engine_from_account_data, header_from_account_data, market_signer_seeds, MARKET_ACCOUNT_SIZE,
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
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

    /// User's token account to receive withdrawn tokens.
    #[account(
        mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

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

pub fn handler(
    ctx: Context<Withdraw>,
    account_idx: u16,
    amount: u64,
    funding_rate: i64,
) -> Result<()> {
    require!(amount > 0, PercolatorError::InsufficientBalance);

    // Validate engine state and execute withdrawal (checks margin requirements)
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    let header = header_from_account_data(&data)?;
    require!(header.mint == ctx.accounts.mint.key(), PercolatorError::Unauthorized);

    let engine = engine_from_account_data(&mut data);

    // Verify signer owns this account
    let account_owner = engine.accounts[account_idx as usize].owner;
    require!(
        account_owner == ctx.accounts.user.key().to_bytes(),
        PercolatorError::Unauthorized
    );

    let oracle_price = engine.last_oracle_price;
    let clock = Clock::get()?;

    // Engine validates margin requirements before allowing withdrawal
    engine
        .withdraw_not_atomic(account_idx, amount as u128, oracle_price, clock.slot, funding_rate)
        .map_err(from_risk_error)?;

    // Drop the borrow before CPI
    drop(data);

    // Transfer tokens from vault to user (signed by market PDA, which is vault authority)
    let bump = [header.bump];
    let signer_seeds = market_signer_seeds(&header.authority, &bump);
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.market.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
            &[&signer_seeds],
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(events::Withdrawn {
        user: ctx.accounts.user.key(),
        account_idx,
        amount,
    });

    Ok(())
}
