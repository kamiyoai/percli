use anchor_lang::prelude::*;

#[error_code]
pub enum PercolatorError {
    #[msg("Insufficient balance for this operation")]
    InsufficientBalance,
    #[msg("Account is undercollateralized")]
    Undercollateralized,
    #[msg("Unauthorized signer")]
    Unauthorized,
    #[msg("Invalid matching engine")]
    InvalidMatchingEngine,
    #[msg("PnL not yet warmed up")]
    PnlNotWarmedUp,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Account not found in engine")]
    AccountNotFound,
    #[msg("Account is not an LP account")]
    NotAnLPAccount,
    #[msg("Position size mismatch")]
    PositionSizeMismatch,
    #[msg("Account kind mismatch")]
    AccountKindMismatch,
    #[msg("Side is blocked (drain-only or reset pending)")]
    SideBlocked,
    #[msg("Engine state is corrupt")]
    CorruptState,
    #[msg("Invalid oracle price")]
    InvalidOraclePrice,
    #[msg("Account index out of range")]
    AccountIndexOutOfRange,
    #[msg("Oracle price is stale")]
    StaleOracle,
    #[msg("Oracle price is negative or zero")]
    InvalidOraclePriceValue,
    #[msg("Deposit amount exceeds u64 token limit")]
    AmountOverflow,
}

pub fn from_risk_error(e: percli_core::RiskError) -> Error {
    match e {
        percli_core::RiskError::InsufficientBalance => {
            error!(PercolatorError::InsufficientBalance)
        }
        percli_core::RiskError::Undercollateralized => {
            error!(PercolatorError::Undercollateralized)
        }
        percli_core::RiskError::Unauthorized => error!(PercolatorError::Unauthorized),
        percli_core::RiskError::InvalidMatchingEngine => {
            error!(PercolatorError::InvalidMatchingEngine)
        }
        percli_core::RiskError::PnlNotWarmedUp => error!(PercolatorError::PnlNotWarmedUp),
        percli_core::RiskError::Overflow => error!(PercolatorError::Overflow),
        percli_core::RiskError::AccountNotFound => error!(PercolatorError::AccountNotFound),
        percli_core::RiskError::NotAnLPAccount => error!(PercolatorError::NotAnLPAccount),
        percli_core::RiskError::PositionSizeMismatch => {
            error!(PercolatorError::PositionSizeMismatch)
        }
        percli_core::RiskError::AccountKindMismatch => {
            error!(PercolatorError::AccountKindMismatch)
        }
        percli_core::RiskError::SideBlocked => error!(PercolatorError::SideBlocked),
        percli_core::RiskError::CorruptState => error!(PercolatorError::CorruptState),
    }
}
