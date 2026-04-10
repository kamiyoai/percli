use anchor_lang::prelude::*;

#[event]
pub struct MarketInitialized {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub oracle: Pubkey,
    pub matcher: Pubkey,
    pub init_slot: u64,
    pub init_oracle_price: u64,
}

#[event]
pub struct Deposited {
    pub user: Pubkey,
    pub account_idx: u16,
    pub amount: u64,
}

#[event]
pub struct Withdrawn {
    pub user: Pubkey,
    pub account_idx: u16,
    pub amount: u64,
}

#[event]
pub struct TradeExecuted {
    pub matcher: Pubkey,
    pub account_a: u16,
    pub account_b: u16,
    pub size_q: i128,
    pub exec_price: u64,
}

#[event]
pub struct Cranked {
    pub cranker: Pubkey,
    pub oracle_price: u64,
    pub slot: u64,
}

#[event]
pub struct Liquidated {
    pub liquidator: Pubkey,
    pub account_idx: u16,
    pub liquidated: bool,
}

#[event]
pub struct Settled {
    pub user: Pubkey,
    pub account_idx: u16,
}

#[event]
pub struct AccountClosed {
    pub user: Pubkey,
    pub account_idx: u16,
}

#[event]
pub struct AccountReclaimed {
    pub reclaimer: Pubkey,
    pub account_idx: u16,
}

#[event]
pub struct InsuranceWithdrawn {
    pub authority: Pubkey,
    pub amount: u64,
}

#[event]
pub struct InsuranceToppedUp {
    pub depositor: Pubkey,
    pub amount: u64,
}

#[event]
pub struct FeeCreditsDeposited {
    pub user: Pubkey,
    pub account_idx: u16,
    pub amount: u64,
}

#[event]
pub struct PnlConverted {
    pub user: Pubkey,
    pub account_idx: u16,
    pub x_req: u64,
}

#[event]
pub struct MarketAccrued {
    pub signer: Pubkey,
    pub oracle_price: u64,
    pub slot: u64,
}

#[event]
pub struct MatcherUpdated {
    pub authority: Pubkey,
    pub old_matcher: Pubkey,
    pub new_matcher: Pubkey,
}

#[event]
pub struct OracleUpdated {
    pub authority: Pubkey,
    pub old_oracle: Pubkey,
    pub new_oracle: Pubkey,
}
