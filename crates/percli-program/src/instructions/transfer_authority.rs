use anchor_lang::prelude::*;

use crate::error::PercolatorError;
use crate::instructions::events;
use crate::state::{header_from_account_data, write_header, MARKET_ACCOUNT_SIZE};

/// Initiate a two-step authority transfer. Writes `new_authority` into
/// `header.pending_authority` but does **not** rotate `header.authority` —
/// that only happens when the pending party calls `accept_authority`.
///
/// Passing `Pubkey::default()` as `new_authority` cancels any in-flight
/// transfer (since `accept_authority` rejects the default pubkey as a signer
/// and explicitly rejects `pending_authority == default`).
///
/// Self-transfer (`new_authority == header.authority`) is rejected with
/// `Unauthorized` because it serves no purpose and would emit a confusing
/// event for indexers.
///
/// Calling this with a different `new_authority` while a transfer is already
/// in flight overwrites the previous `pending_authority` — this is intentional
/// (it lets the authority change their mind) and emits
/// `AuthorityTransferInitiated` for the new pending key.
#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Validated via owner, discriminator, and size.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
        constraint = market.data_len() >= MARKET_ACCOUNT_SIZE @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<TransferAuthority>, new_authority: Pubkey) -> Result<()> {
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
    // Reject self-transfer: it's a no-op and only adds event noise.
    require!(
        new_authority != header.authority,
        PercolatorError::Unauthorized
    );

    let previous_pending = header.pending_authority;
    header.pending_authority = new_authority;
    write_header(&mut data, &header);

    if new_authority == Pubkey::default() {
        emit!(events::AuthorityTransferCancelled {
            market: market.key(),
            authority: header.authority,
            previous_pending,
        });
    } else {
        emit!(events::AuthorityTransferInitiated {
            market: market.key(),
            old_authority: header.authority,
            pending_authority: new_authority,
        });
    }

    Ok(())
}
