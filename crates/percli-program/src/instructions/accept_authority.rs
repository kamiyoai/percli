use anchor_lang::prelude::*;

use crate::error::PercolatorError;
use crate::instructions::events;
use crate::state::{header_from_account_data, write_header, MARKET_ACCOUNT_SIZE};

/// Complete a two-step authority transfer. The caller must match
/// `header.pending_authority`, which must not be `Pubkey::default()`.
///
/// On success:
///   - `header.authority` becomes the new authority.
///   - `header.pending_authority` is reset to `Pubkey::default()`.
///   - `AuthorityAccepted` is emitted.
///
/// The two-step design prevents the authority from being accidentally handed
/// to an invalid / unreachable pubkey, which would permanently brick the
/// market.
#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    pub new_authority: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<AcceptAuthority>) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    require!(
        crate::state::is_v1_market(&data),
        PercolatorError::AccountNotFound
    );

    // Defense in depth: the Solana runtime already rejects `Pubkey::default()`
    // as a signer (it has no corresponding private key), so this check is
    // theoretically unreachable. We keep it because it makes the security
    // intent legible inside the handler and protects against any future
    // runtime change.
    require!(
        ctx.accounts.new_authority.key() != Pubkey::default(),
        PercolatorError::Unauthorized
    );

    let mut header = header_from_account_data(&data)?;
    require!(
        header.pending_authority != Pubkey::default(),
        PercolatorError::NoPendingAuthority
    );
    require!(
        header.pending_authority == ctx.accounts.new_authority.key(),
        PercolatorError::Unauthorized
    );

    let old_authority = header.authority;
    let new_authority = header.pending_authority;
    header.authority = new_authority;
    header.pending_authority = Pubkey::default();
    write_header(&mut data, &header);

    emit!(events::AuthorityAccepted {
        market: market.key(),
        old_authority,
        new_authority,
    });

    Ok(())
}
