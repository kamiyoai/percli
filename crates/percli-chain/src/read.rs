use anyhow::{bail, Result};
use percli_core::RiskEngine;

/// Discriminator expected at bytes 0..8 of the market account.
const MARKET_DISCRIMINATOR: &[u8; 8] = b"percmrkt";

/// MarketHeader size (authority: 32 + mint: 32 + oracle: 32 + bump: 1 + vault_bump: 1 + padding: 6 = 104).
const HEADER_SIZE: usize = 104;

/// Offset where RiskEngine data begins in the account.
const ENGINE_OFFSET: usize = 8 + HEADER_SIZE;

/// Minimum account data size for a valid market.
pub fn min_account_size() -> usize {
    ENGINE_OFFSET + std::mem::size_of::<RiskEngine>()
}

/// Read the authority pubkey from raw market account data.
pub fn read_authority(data: &[u8]) -> Result<[u8; 32]> {
    if data.len() < ENGINE_OFFSET {
        bail!("Account data too small for market header");
    }
    // Validate discriminator
    if &data[..8] != MARKET_DISCRIMINATOR {
        bail!(
            "Invalid market discriminator: expected {:?}, got {:?}",
            MARKET_DISCRIMINATOR,
            &data[..8]
        );
    }
    let mut authority = [0u8; 32];
    authority.copy_from_slice(&data[8..40]);
    Ok(authority)
}

/// Get a reference to the RiskEngine from raw market account data.
///
/// # Safety
/// The data must be at least `min_account_size()` bytes and the engine
/// region must be properly aligned. RiskEngine is `#[repr(C)]` with
/// all-valid bit patterns.
pub fn engine_from_data(data: &[u8]) -> Result<&RiskEngine> {
    if data.len() < min_account_size() {
        bail!(
            "Account data too small: {} < {}",
            data.len(),
            min_account_size()
        );
    }
    if &data[..8] != MARKET_DISCRIMINATOR {
        bail!("Invalid market discriminator");
    }
    let engine_bytes = &data[ENGINE_OFFSET..ENGINE_OFFSET + std::mem::size_of::<RiskEngine>()];
    // SAFETY: RiskEngine is #[repr(C)] with all-valid bit patterns (all fields are
    // integers or arrays of integers). The slice is correctly sized.
    Ok(unsafe { &*(engine_bytes.as_ptr() as *const RiskEngine) })
}
