use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use percli_core::RiskParams;

use crate::error::PercolatorError;
use crate::state::{engine_from_account_data, write_header, MarketHeader, MARKET_ACCOUNT_SIZE};

#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Manually validated. Initialized as a PDA with the correct size.
    #[account(
        init,
        payer = authority,
        space = MARKET_ACCOUNT_SIZE,
        seeds = [b"market", authority.key().as_ref()],
        bump,
    )]
    pub market: UncheckedAccount<'info>,

    /// The SPL token mint for this market's collateral (e.g. USDC).
    pub mint: Account<'info, Mint>,

    /// Vault token account — holds all deposited collateral for this market.
    #[account(
        init,
        payer = authority,
        token::mint = mint,
        token::authority = market,
        seeds = [b"vault", market.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeMarket>,
    init_slot: u64,
    init_oracle_price: u64,
    params: RiskParamsInput,
) -> Result<()> {
    let market = &ctx.accounts.market;
    let mut data = market.try_borrow_mut_data()?;

    // Write discriminator (first 8 bytes) — use a fixed marker
    data[0..8].copy_from_slice(b"percmrkt");

    let header = MarketHeader {
        authority: ctx.accounts.authority.key(),
        mint: ctx.accounts.mint.key(),
        bump: ctx.bumps.market,
        vault_bump: ctx.bumps.vault,
        _padding: [0; 6],
    };
    write_header(&mut data, &header);

    require!(init_oracle_price > 0, PercolatorError::InvalidOraclePriceValue);

    let engine = engine_from_account_data(&mut data);
    let risk_params = params.to_risk_params();
    engine.init_in_place(risk_params, init_slot, init_oracle_price);

    Ok(())
}

/// Anchor-friendly input for RiskParams (all primitives, no wrapper types).
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RiskParamsInput {
    pub warmup_period_slots: u64,
    pub maintenance_margin_bps: u64,
    pub initial_margin_bps: u64,
    pub trading_fee_bps: u64,
    pub max_accounts: u64,
    pub new_account_fee: u64,
    pub maintenance_fee_per_slot: u64,
    pub max_crank_staleness_slots: u64,
    pub liquidation_fee_bps: u64,
    pub liquidation_fee_cap: u64,
    pub min_liquidation_abs: u64,
    pub min_initial_deposit: u64,
    pub min_nonzero_mm_req: u64,
    pub min_nonzero_im_req: u64,
    pub insurance_floor: u64,
}

impl RiskParamsInput {
    pub fn to_risk_params(&self) -> RiskParams {
        use percli_core::U128;
        RiskParams {
            warmup_period_slots: self.warmup_period_slots,
            maintenance_margin_bps: self.maintenance_margin_bps,
            initial_margin_bps: self.initial_margin_bps,
            trading_fee_bps: self.trading_fee_bps,
            max_accounts: self.max_accounts,
            new_account_fee: U128::new(self.new_account_fee as u128),
            maintenance_fee_per_slot: U128::new(self.maintenance_fee_per_slot as u128),
            max_crank_staleness_slots: self.max_crank_staleness_slots,
            liquidation_fee_bps: self.liquidation_fee_bps,
            liquidation_fee_cap: U128::new(self.liquidation_fee_cap as u128),
            min_liquidation_abs: U128::new(self.min_liquidation_abs as u128),
            min_initial_deposit: U128::new(self.min_initial_deposit as u128),
            min_nonzero_mm_req: self.min_nonzero_mm_req as u128,
            min_nonzero_im_req: self.min_nonzero_im_req as u128,
            insurance_floor: U128::new(self.insurance_floor as u128),
        }
    }
}
