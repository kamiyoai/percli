use anchor_lang::prelude::*;
use percli_core::RiskEngine;

use crate::error::PercolatorError;
use crate::instructions::events;
use crate::state::{write_header, MarketHeader};

/// One-time migration from the v0 (136-byte) `MarketHeader` layout used by
/// percli v0.9.x to the v1 (168-byte) layout used by percli v1.0+.
///
/// Detection uses a **version byte at offset [7]** of the discriminator:
///   - v0: `b"percmrkt"` (last byte = `0x74`, ASCII `t`)
///   - v1: `b"percmrk\x01"` (last byte = `0x01`)
///
/// We can't use absolute size comparisons because host (`size_of::<RiskEngine>()`
/// at host compile time) and SBF (`size_of` at on-chain compile time) disagree
/// on the size of `RiskEngine` due to platform-specific alignment of `i128`/
/// `u128` and the large `[Account; MAX_ACCOUNTS]` array. The discrepancy is
/// constant (~536 bytes), but it makes any "expected total size" comparison
/// fragile across host/SBF boundaries.
///
/// Migration is performed **in-place without `realloc`**: we shift the engine
/// bytes forward by 32 bytes (from `[144..)` to `[176..)`) inside the existing
/// account buffer. This works because real v0.9 mainnet accounts were created
/// with the v0.9 host-side constant `8 + 136 + size_host(RiskEngine)`, which
/// is *strictly larger* than the SBF v1 size `8 + 168 + size_sbf(RiskEngine)`,
/// so the existing buffer already has enough room.
///
/// This instruction:
///   1. Verifies the account is owned by this program.
///   2. Verifies the discriminator at `[0..7]` is `b"percmrk"`.
///   3. Verifies the version byte at `[7]` is `0x74` (`t`, the v0 marker).
///   4. Verifies the signer matches the authority encoded in the v0 header.
///   5. Shifts the engine bytes forward by 32 bytes (back-to-front via
///      `copy_within`).
///   6. Writes a fresh v1 header at `[8..176)` with `pending_authority =
///      Pubkey::default()` (all other fields copied from the v0 header).
///   7. Stamps the version byte at `[7]` to `0x01`.
///   8. Emits `HeaderMigrated`.
///
/// `migrate_header_v1` is idempotent-by-rejection: calling it a second time
/// fails the version-byte check with `AlreadyMigrated`.
#[derive(Accounts)]
pub struct MigrateHeaderV1<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Manually validated — owner, discriminator/version-byte, and
    /// authority match are all checked inside the handler. We deliberately
    /// don't enforce a `seeds`/`bump` constraint here because we re-derive
    /// and verify the PDA bump inside the handler against the v0 header bytes,
    /// which is the same security guarantee with explicit error reporting.
    #[account(
        mut,
        owner = crate::ID @ PercolatorError::AccountNotFound,
    )]
    pub market: UncheckedAccount<'info>,
}

/// Parse the fields of a v0 MarketHeader out of a byte slice.
///
/// The v0 layout at offset 8 (after discriminator) is:
///   authority (32) | mint (32) | oracle (32) | matcher (32) |
///   bump (1) | vault_bump (1) | _padding (6)
/// …followed immediately by the engine at offset `8 + 136 = 144`.
struct V0Fields {
    authority: Pubkey,
    mint: Pubkey,
    oracle: Pubkey,
    matcher: Pubkey,
    bump: u8,
    vault_bump: u8,
}

fn read_v0_fields(data: &[u8]) -> V0Fields {
    let mut a = [0u8; 32];
    let mut m = [0u8; 32];
    let mut o = [0u8; 32];
    let mut mt = [0u8; 32];
    a.copy_from_slice(&data[8..40]);
    m.copy_from_slice(&data[40..72]);
    o.copy_from_slice(&data[72..104]);
    mt.copy_from_slice(&data[104..136]);
    V0Fields {
        authority: Pubkey::new_from_array(a),
        mint: Pubkey::new_from_array(m),
        oracle: Pubkey::new_from_array(o),
        matcher: Pubkey::new_from_array(mt),
        bump: data[136],
        vault_bump: data[137],
    }
}

pub fn handler(ctx: Context<MigrateHeaderV1>) -> Result<()> {
    let market_info = ctx.accounts.market.to_account_info();

    // -----------------------------------------------------------------------
    // 1. Validate the discriminator/version, then read & verify v0 fields.
    // -----------------------------------------------------------------------
    let v0 = {
        let data = market_info.try_borrow_data()?;
        require!(
            data.len() >= 8 + MarketHeader::SIZE + std::mem::size_of::<RiskEngine>(),
            PercolatorError::AccountNotFound
        );
        require!(
            &data[0..7] == b"percmrk",
            PercolatorError::AccountNotFound
        );
        // Version byte: v0 = 0x74 ('t'), v1 = 0x01.
        // The order of these two checks matters for error reporting:
        // a non-v0, non-v1 byte falls through to NotLegacyLayout (correct).
        require!(data[7] != 0x01, PercolatorError::AlreadyMigrated);
        require!(data[7] == 0x74, PercolatorError::NotLegacyLayout);
        read_v0_fields(&data)
    };
    require!(
        v0.authority == ctx.accounts.authority.key(),
        PercolatorError::Unauthorized
    );

    // Re-derive the Market PDA from the v0-encoded authority and verify the
    // bump. This catches any account whose v0 header was tampered with (or
    // simply corrupted) such that the stored bump no longer matches the
    // canonical PDA. We don't validate `vault_bump` here — the vault is a
    // separate token account that isn't passed to this instruction; the next
    // instruction that touches the vault (e.g. `deposit`) will revalidate it
    // via the Anchor `seeds`/`bump` constraint.
    let (expected_market, expected_bump) =
        Pubkey::find_program_address(&[b"market", v0.authority.as_ref()], &crate::ID);
    require!(
        expected_market == market_info.key(),
        PercolatorError::AccountNotFound
    );
    require!(
        expected_bump == v0.bump,
        PercolatorError::CorruptState
    );

    // -----------------------------------------------------------------------
    // 2. Shift the engine bytes forward by 32 (in-place, no realloc).
    //
    // Before the shift:
    //   [0..8)               discriminator (`percmrkt`)
    //   [8..144)             old v0 header bytes
    //   [144..144 + E)       engine bytes (E = SBF size_of::<RiskEngine>())
    //   [144 + E..)          slack tail bytes left over from host create_account
    //
    // After the shift:
    //   [0..8)               discriminator (we'll restamp byte [7] = 0x01)
    //   [8..176)             stale bytes — overwritten by `write_header` next
    //   [176..176 + E)       engine bytes in their v1 location
    //   [176 + E..)          slack tail (unchanged)
    // -----------------------------------------------------------------------
    let mut data = market_info.try_borrow_mut_data()?;
    let engine_size = std::mem::size_of::<RiskEngine>();
    let old_engine_start: usize = 8 + MarketHeader::SIZE_V0; // 144
    let new_engine_start: usize = 8 + MarketHeader::SIZE; // 176

    // Sanity: the existing buffer must have room for the v1 layout. Real
    // v0.9 mainnet accounts always do (host create_account size > SBF v1
    // size by ~536 bytes due to platform alignment differences in
    // RiskEngine), and integration tests likewise allocate a buffer of
    // host-side `MARKET_ACCOUNT_SIZE_V0`.
    require!(
        data.len() >= new_engine_start + engine_size,
        PercolatorError::AccountNotFound
    );

    // `copy_within` handles overlapping source/destination correctly. Since
    // we're copying from a lower offset to a higher one, it iterates
    // back-to-front internally to avoid clobbering.
    data.copy_within(
        old_engine_start..(old_engine_start + engine_size),
        new_engine_start,
    );

    // -----------------------------------------------------------------------
    // 3. Write the v1 header. `write_header` overwrites bytes [8..176).
    //    Then stamp the version byte at [7] = 0x01.
    // -----------------------------------------------------------------------
    let header = MarketHeader {
        authority: v0.authority,
        mint: v0.mint,
        oracle: v0.oracle,
        matcher: v0.matcher,
        pending_authority: Pubkey::default(),
        bump: v0.bump,
        vault_bump: v0.vault_bump,
        _padding: [0; 6],
    };
    write_header(&mut data, &header);
    data[7] = 0x01;

    let actual_account_size = data.len() as u64;
    drop(data);

    emit!(events::HeaderMigrated {
        authority: ctx.accounts.authority.key(),
        market: market_info.key(),
        mint: v0.mint,
        oracle: v0.oracle,
        matcher: v0.matcher,
        account_size: actual_account_size,
    });

    Ok(())
}
