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
}
