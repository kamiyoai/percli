#![allow(unexpected_cfgs, clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("PercQhVBxXnVCaAhfrPZFc2dVZcQANnwEYroogLJFwm");

#[program]
pub mod percli_program {
    use super::*;

    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        init_slot: u64,
        init_oracle_price: u64,
        params: RiskParamsInput,
    ) -> Result<()> {
        instructions::initialize_market::handler(ctx, init_slot, init_oracle_price, params)
    }

    pub fn deposit(ctx: Context<Deposit>, account_idx: u16, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, account_idx, amount)
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        account_idx: u16,
        amount: u64,
        funding_rate: i64,
    ) -> Result<()> {
        instructions::withdraw::handler(ctx, account_idx, amount, funding_rate)
    }

    pub fn trade(
        ctx: Context<Trade>,
        account_a: u16,
        account_b: u16,
        size_q: i128,
        exec_price: u64,
        funding_rate: i64,
    ) -> Result<()> {
        instructions::trade::handler(ctx, account_a, account_b, size_q, exec_price, funding_rate)
    }

    pub fn crank(ctx: Context<Crank>, funding_rate: i64) -> Result<()> {
        instructions::crank::handler(ctx, funding_rate)
    }

    pub fn liquidate(ctx: Context<Liquidate>, account_idx: u16, funding_rate: i64) -> Result<bool> {
        instructions::liquidate::handler(ctx, account_idx, funding_rate)
    }

    pub fn settle(ctx: Context<Settle>, account_idx: u16, funding_rate: i64) -> Result<()> {
        instructions::settle::handler(ctx, account_idx, funding_rate)
    }

    pub fn close_account(
        ctx: Context<CloseAccount>,
        account_idx: u16,
        funding_rate: i64,
    ) -> Result<()> {
        instructions::close_account::handler(ctx, account_idx, funding_rate)
    }

    pub fn reclaim_account(ctx: Context<ReclaimAccount>, account_idx: u16) -> Result<()> {
        instructions::reclaim_account::handler(ctx, account_idx)
    }

    pub fn withdraw_insurance(ctx: Context<WithdrawInsurance>, amount: u64) -> Result<()> {
        instructions::withdraw_insurance::handler(ctx, amount)
    }

    pub fn top_up_insurance(ctx: Context<TopUpInsurance>, amount: u64) -> Result<()> {
        instructions::top_up_insurance::handler(ctx, amount)
    }

    pub fn deposit_fee_credits(
        ctx: Context<DepositFeeCredits>,
        account_idx: u16,
        amount: u64,
    ) -> Result<()> {
        instructions::deposit_fee_credits::handler(ctx, account_idx, amount)
    }

    pub fn convert_released_pnl(
        ctx: Context<ConvertReleasedPnl>,
        account_idx: u16,
        x_req: u64,
        funding_rate: i64,
    ) -> Result<()> {
        instructions::convert_released_pnl::handler(ctx, account_idx, x_req, funding_rate)
    }

    pub fn accrue_market(ctx: Context<AccrueMarket>) -> Result<()> {
        instructions::accrue_market::handler(ctx)
    }

    pub fn update_matcher(ctx: Context<UpdateMatcher>, new_matcher: Pubkey) -> Result<()> {
        instructions::update_matcher::handler(ctx, new_matcher)
    }

    pub fn update_oracle(ctx: Context<UpdateOracle>) -> Result<()> {
        instructions::update_oracle::handler(ctx)
    }
}
