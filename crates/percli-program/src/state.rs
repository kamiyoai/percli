use anchor_lang::prelude::*;
use percli_core::RiskEngine;

/// Market account header (v1.0 layout) — stored at the beginning of the account data.
/// The RiskEngine state follows immediately after, accessed via raw pointer
/// because RiskEngine is ~1.165 MB and doesn't derive Copy.
///
/// Layout version: v1 (introduced in percli v1.0.0). v0.9.x markets used a 136-byte
/// header without `pending_authority` — those accounts must be migrated via
/// `migrate_header_v1` before any other instruction can run against them.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct MarketHeader {
    /// Authority that created this market (can update params, rotate keys).
    pub authority: Pubkey,
    /// SPL token mint for this market's collateral.
    pub mint: Pubkey,
    /// Pyth price feed account for this market's oracle.
    pub oracle: Pubkey,
    /// Matcher authority — the only signer allowed to submit trades.
    pub matcher: Pubkey,
    /// Pending authority for two-step transfer. `Pubkey::default()` when no
    /// transfer is in flight. Only the holder of this key can call
    /// `accept_authority` to complete the handoff.
    pub pending_authority: Pubkey,
    /// Bump seed for the Market PDA.
    pub bump: u8,
    /// Bump seed for the vault token account PDA.
    pub vault_bump: u8,
    /// Padding for 8-byte alignment.
    pub _padding: [u8; 6],
}

impl MarketHeader {
    /// Current (v1) header size: 4×Pubkey + pending_authority + 2 bumps + 6 padding = 168 bytes.
    pub const SIZE: usize = 32 + 32 + 32 + 32 + 32 + 1 + 1 + 6; // 168 bytes
    /// Legacy (v0) header size — used only by `migrate_header_v1` to detect
    /// pre-v1.0 accounts that still need to be expanded by 32 bytes.
    pub const SIZE_V0: usize = 32 + 32 + 32 + 32 + 1 + 1 + 6; // 136 bytes
}

/// 8-byte discriminator for v1 market accounts: 7 fixed bytes (`b"percmrk"`)
/// followed by a 1-byte layout version (`0x01`).
///
/// Note: percli v0.9 accounts use `b"percmrkt"` (last byte = `0x74` = `'t'`),
/// the legacy v0 marker. They must be migrated via `migrate_header_v1` before
/// any other instruction can run against them.
pub const MARKET_DISCRIMINATOR_V1: [u8; 8] = *b"percmrk\x01";

/// Returns `true` iff `data` starts with the v1 discriminator.
#[inline]
pub fn is_v1_market(data: &[u8]) -> bool {
    data.len() >= 8 && data[0..8] == MARKET_DISCRIMINATOR_V1
}

/// Total account size (v1 layout): discriminator + header + engine
pub const MARKET_ACCOUNT_SIZE: usize = 8 + MarketHeader::SIZE + std::mem::size_of::<RiskEngine>();

/// Total account size for the legacy v0.9 layout. Used by `migrate_header_v1`
/// to validate that the account being migrated is a pre-v1 market.
pub const MARKET_ACCOUNT_SIZE_V0: usize = 8 + MarketHeader::SIZE_V0 + std::mem::size_of::<RiskEngine>();

/// Market PDA signer seeds: [b"market", authority_key, &[bump]]
pub fn market_signer_seeds<'a>(
    authority: &'a Pubkey,
    bump: &'a [u8; 1],
) -> [&'a [u8]; 3] {
    [b"market", authority.as_ref(), bump.as_ref()]
}

/// Helper to get a mutable reference to the RiskEngine from raw account data.
///
/// SAFETY: The caller must ensure the account data is at least
/// `8 + MarketHeader::SIZE + size_of::<RiskEngine>()` bytes and properly
/// initialized. The RiskEngine is #[repr(C)] with all-valid bit patterns.
pub fn engine_from_account_data(data: &mut [u8]) -> &mut RiskEngine {
    let offset = 8 + MarketHeader::SIZE;
    let engine_bytes = &mut data[offset..offset + std::mem::size_of::<RiskEngine>()];
    unsafe { &mut *(engine_bytes.as_mut_ptr() as *mut RiskEngine) }
}

pub fn engine_from_account_data_ref(data: &[u8]) -> &RiskEngine {
    let offset = 8 + MarketHeader::SIZE;
    let engine_bytes = &data[offset..offset + std::mem::size_of::<RiskEngine>()];
    unsafe { &*(engine_bytes.as_ptr() as *const RiskEngine) }
}

pub fn header_from_account_data(data: &[u8]) -> std::result::Result<MarketHeader, anchor_lang::error::Error> {
    let header_bytes = &data[8..8 + MarketHeader::SIZE];
    MarketHeader::try_from_slice(header_bytes)
        .map_err(|_| anchor_lang::error!(crate::error::PercolatorError::CorruptState))
}

pub fn write_header(data: &mut [u8], header: &MarketHeader) {
    let mut cursor = &mut data[8..8 + MarketHeader::SIZE];
    header.serialize(&mut cursor).unwrap();
}
