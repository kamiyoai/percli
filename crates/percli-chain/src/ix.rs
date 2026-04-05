use borsh::BorshSerialize;
use sha2::{Digest, Sha256};
#[allow(deprecated)]
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

/// Compute the Anchor instruction discriminator: first 8 bytes of sha256("global:<name>").
pub fn anchor_discriminator(name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

/// Build an Anchor instruction with discriminator + borsh-serialized args.
pub fn build_ix(
    program_id: &Pubkey,
    name: &str,
    args: impl BorshSerialize,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let mut data = anchor_discriminator(name).to_vec();
    args.serialize(&mut data).expect("borsh serialize");
    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

// --- Arg structs matching the on-chain Anchor ABI ---

#[derive(BorshSerialize)]
pub struct InitializeMarketArgs {
    pub init_slot: u64,
    pub init_oracle_price: u64,
    pub params: RiskParamsInput,
}

#[derive(BorshSerialize)]
pub struct RiskParamsInput {
    pub maintenance_margin_bps: u64,
    pub initial_margin_bps: u64,
    pub liquidation_fee_bps: u64,
    pub insurance_fee_bps: u64,
    pub adl_threshold_bps: u64,
    pub max_leverage_q: u64,
    pub min_order_size_q: u64,
    pub tick_size: u64,
    pub max_oi_imbalance_q: u64,
    pub haircut_start_bps: u64,
    pub haircut_end_bps: u64,
    pub haircut_maturity_slots: u64,
    pub matured_haircut_bps: u64,
    pub max_accounts: u64,
    pub min_nonzero_mm_req: u128,
    pub min_nonzero_im_req: u128,
}

#[derive(BorshSerialize)]
pub struct DepositArgs {
    pub account_idx: u16,
    pub amount: u128,
}

#[derive(BorshSerialize)]
pub struct WithdrawArgs {
    pub account_idx: u16,
    pub amount: u128,
    pub funding_rate: i64,
}

#[derive(BorshSerialize)]
pub struct TradeArgs {
    pub account_a: u16,
    pub account_b: u16,
    pub size_q: i128,
    pub exec_price: u64,
    pub funding_rate: i64,
}

#[derive(BorshSerialize)]
pub struct CrankArgs {
    pub oracle_price: u64,
    pub funding_rate: i64,
}

#[derive(BorshSerialize)]
pub struct LiquidateArgs {
    pub account_idx: u16,
    pub funding_rate: i64,
}

#[derive(BorshSerialize)]
pub struct SettleArgs {
    pub account_idx: u16,
    pub funding_rate: i64,
}

#[derive(BorshSerialize)]
pub struct CloseAccountArgs {
    pub account_idx: u16,
    pub funding_rate: i64,
}

// --- Instruction builders ---

pub fn initialize_market_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    authority: &Pubkey,
    args: InitializeMarketArgs,
) -> Instruction {
    build_ix(
        program_id,
        "initialize_market",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    )
}

pub fn deposit_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    user: &Pubkey,
    args: DepositArgs,
) -> Instruction {
    build_ix(
        program_id,
        "deposit",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*user, true),
        ],
    )
}

pub fn withdraw_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    user: &Pubkey,
    args: WithdrawArgs,
) -> Instruction {
    build_ix(
        program_id,
        "withdraw",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*user, true),
        ],
    )
}

pub fn trade_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    authority: &Pubkey,
    args: TradeArgs,
) -> Instruction {
    build_ix(
        program_id,
        "trade",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
        ],
    )
}

pub fn crank_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    cranker: &Pubkey,
    args: CrankArgs,
) -> Instruction {
    build_ix(
        program_id,
        "crank",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*cranker, true),
        ],
    )
}

pub fn liquidate_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    liquidator: &Pubkey,
    args: LiquidateArgs,
) -> Instruction {
    build_ix(
        program_id,
        "liquidate",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*liquidator, true),
        ],
    )
}

pub fn settle_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    user: &Pubkey,
    args: SettleArgs,
) -> Instruction {
    build_ix(
        program_id,
        "settle",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*user, true),
        ],
    )
}

pub fn close_account_ix(
    program_id: &Pubkey,
    market: &Pubkey,
    user: &Pubkey,
    args: CloseAccountArgs,
) -> Instruction {
    build_ix(
        program_id,
        "close_account",
        args,
        vec![
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*user, true),
        ],
    )
}
