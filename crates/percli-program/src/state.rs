use anchor_lang::prelude::*;
use percli_core::RiskEngine;

/// Market account header — stored at the beginning of the account data.
/// The RiskEngine state follows immediately after, accessed via raw pointer
/// because RiskEngine is ~1.165 MB and doesn't derive Copy.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct MarketHeader {
    /// Authority that created this market (can update params).
    pub authority: Pubkey,
    /// SPL token mint for this market's collateral.
    pub mint: Pubkey,
    /// Pyth price feed account for this market's oracle.
    pub oracle: Pubkey,
    /// Matcher authority — the only signer allowed to submit trades.
    pub matcher: Pubkey,
    /// Bump seed for the Market PDA.
    pub bump: u8,
    /// Bump seed for the vault token account PDA.
    pub vault_bump: u8,
    /// Padding for 8-byte alignment.
    pub _padding: [u8; 6],
}

impl MarketHeader {
    pub const SIZE: usize = 32 + 32 + 32 + 32 + 1 + 1 + 6; // 136 bytes
}

/// Total account size: discriminator + header + engine
pub const MARKET_ACCOUNT_SIZE: usize = 8 + MarketHeader::SIZE + std::mem::size_of::<RiskEngine>();

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
