#![allow(unexpected_cfgs)]
//! percli-core: Offline simulation adapter for the Percolator risk engine.
//!
//! Wraps the Percolator engine with human-readable account names,
//! serde-compatible snapshots, and ergonomic defaults.

#[cfg(not(target_os = "solana"))]
pub mod agent;
pub mod scenario;

pub use percolator::{
    Account, CrankOutcome, InsuranceFund, LiquidationPolicy, RiskEngine, RiskError, RiskParams,
    SideMode, ADL_ONE, I128, MAX_ACCOUNTS, MAX_ORACLE_PRICE, MAX_VAULT_TVL, POS_SCALE, U128,
};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Human-readable wrapper around the Percolator [`RiskEngine`].
///
/// Maps string account names to u16 indices, tracks oracle/slot/funding state,
/// and provides ergonomic methods for deposits, withdrawals, trades, and cranks.
pub struct NamedEngine {
    pub engine: RiskEngine,
    pub accounts: BTreeMap<String, u16>,
    pub current_slot: u64,
    pub last_oracle_price: u64,
    pub last_funding_rate: i64,
    next_idx: u16,
}

impl NamedEngine {
    /// Create a new engine with the given risk parameters, slot, and oracle price.
    pub fn new(params: RiskParams, init_slot: u64, init_oracle_price: u64) -> Self {
        let engine = RiskEngine::new_with_market(params, init_slot, init_oracle_price);
        Self {
            engine,
            accounts: BTreeMap::new(),
            current_slot: init_slot,
            last_oracle_price: init_oracle_price,
            last_funding_rate: 0,
            next_idx: 0,
        }
    }

    /// Resolve an account name to its index. Returns error if not found.
    pub fn resolve(&self, name: &str) -> Result<u16, EngineError> {
        self.accounts
            .get(name)
            .copied()
            .ok_or_else(|| EngineError::UnknownAccount(name.to_string()))
    }

    fn ensure_account(&mut self, name: &str) -> Result<u16, EngineError> {
        if let Some(&idx) = self.accounts.get(name) {
            return Ok(idx);
        }
        if name.is_empty() || name.len() > 64 {
            return Err(EngineError::InvalidAccountName(name.to_string()));
        }
        let limit = self.engine.params.max_accounts as usize;
        if (self.next_idx as usize) >= limit {
            return Err(EngineError::AccountLimitReached(limit));
        }
        let idx = self.next_idx;
        self.next_idx += 1;
        self.accounts.insert(name.to_string(), idx);
        Ok(idx)
    }

    /// Deposit collateral into an account. Creates the account if it doesn't exist.
    pub fn deposit(&mut self, name: &str, amount: u128) -> Result<(), EngineError> {
        if amount == 0 {
            return Err(EngineError::InvalidAmount);
        }
        let idx = self.ensure_account(name)?;
        self.engine
            .deposit(idx, amount, self.last_oracle_price, self.current_slot)
            .map_err(EngineError::Risk)
    }

    /// Withdraw collateral from an account. Fails if the account would breach margin.
    pub fn withdraw(&mut self, name: &str, amount: u128) -> Result<(), EngineError> {
        if amount == 0 {
            return Err(EngineError::InvalidAmount);
        }
        let idx = self.resolve(name)?;
        self.engine
            .withdraw_not_atomic(
                idx,
                amount,
                self.last_oracle_price,
                self.current_slot,
                self.last_funding_rate,
            )
            .map_err(EngineError::Risk)
    }

    /// Execute a trade between two accounts. `size_q` is in POS_SCALE units.
    pub fn trade(
        &mut self,
        long: &str,
        short: &str,
        size_q: i128,
        exec_price: u64,
    ) -> Result<(), EngineError> {
        let a = self.resolve(long)?;
        let b = self.resolve(short)?;
        self.engine
            .execute_trade_not_atomic(
                a,
                b,
                self.last_oracle_price,
                self.current_slot,
                size_q,
                exec_price,
                self.last_funding_rate,
            )
            .map_err(EngineError::Risk)
    }

    /// Run the keeper crank at the given oracle price and slot.
    pub fn crank(&mut self, oracle_price: u64, slot: u64) -> Result<CrankOutcome, EngineError> {
        self.last_oracle_price = oracle_price;
        self.current_slot = slot;
        self.engine
            .keeper_crank_not_atomic(slot, oracle_price, &[], 0, self.last_funding_rate)
            .map_err(EngineError::Risk)
    }

    /// Attempt to liquidate an account. Returns true if liquidation occurred.
    pub fn liquidate(&mut self, name: &str) -> Result<bool, EngineError> {
        let idx = self.resolve(name)?;
        self.engine
            .liquidate_at_oracle_not_atomic(
                idx,
                self.current_slot,
                self.last_oracle_price,
                LiquidationPolicy::FullClose,
                self.last_funding_rate,
            )
            .map_err(EngineError::Risk)
    }

    /// Settle an account's PnL against the vault.
    pub fn settle(&mut self, name: &str) -> Result<(), EngineError> {
        let idx = self.resolve(name)?;
        self.engine
            .settle_account_not_atomic(
                idx,
                self.last_oracle_price,
                self.current_slot,
                self.last_funding_rate,
            )
            .map_err(EngineError::Risk)
    }

    pub fn set_oracle(&mut self, oracle_price: u64) -> Result<(), EngineError> {
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(EngineError::InvalidOraclePrice(oracle_price));
        }
        self.last_oracle_price = oracle_price;
        Ok(())
    }

    pub fn set_slot(&mut self, slot: u64) {
        self.current_slot = slot;
    }

    pub fn set_funding_rate(&mut self, rate: i64) {
        self.last_funding_rate = rate;
    }

    /// Capture a complete snapshot of the engine state.
    pub fn snapshot(&self) -> EngineSnapshot {
        let (h_num, h_den) = self.engine.haircut_ratio();
        let conservation = self.engine.check_conservation();

        let mut account_snaps = Vec::new();
        for (name, &idx) in &self.accounts {
            if self.engine.is_used(idx as usize) {
                let acct = &self.engine.accounts[idx as usize];
                let eff_pos = self.engine.effective_pos_q(idx as usize);
                let eff_pnl = self.engine.effective_matured_pnl(idx as usize);
                let equity_maint = self.engine.account_equity_maint_raw(acct);
                let equity_init = self.engine.account_equity_init_raw(acct, idx as usize);
                let above_mm = self.engine.is_above_maintenance_margin(
                    acct,
                    idx as usize,
                    self.last_oracle_price,
                );
                let above_im =
                    self.engine
                        .is_above_initial_margin(acct, idx as usize, self.last_oracle_price);
                let notional = self.engine.notional(idx as usize, self.last_oracle_price);
                account_snaps.push(AccountSnapshot {
                    name: name.clone(),
                    index: idx,
                    capital: acct.capital.get(),
                    pnl: acct.pnl,
                    reserved_pnl: acct.reserved_pnl,
                    effective_position_q: eff_pos,
                    effective_matured_pnl: eff_pnl,
                    equity_maint,
                    equity_init,
                    above_maintenance_margin: above_mm,
                    above_initial_margin: above_im,
                    notional,
                    fee_credits: acct.fee_credits.get(),
                });
            }
        }

        EngineSnapshot {
            vault: self.engine.vault.get(),
            insurance_fund: self.engine.insurance_fund.balance.get(),
            haircut_numerator: h_num,
            haircut_denominator: h_den,
            conservation,
            current_slot: self.current_slot,
            last_oracle_price: self.last_oracle_price,
            last_funding_rate: self.last_funding_rate,
            side_mode_long: format!("{:?}", self.engine.side_mode_long),
            side_mode_short: format!("{:?}", self.engine.side_mode_short),
            oi_long_q: self.engine.oi_eff_long_q,
            oi_short_q: self.engine.oi_eff_short_q,
            lifetime_liquidations: self.engine.lifetime_liquidations,
            accounts: account_snaps,
        }
    }
}

/// Complete serializable snapshot of the engine state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub vault: u128,
    pub insurance_fund: u128,
    pub haircut_numerator: u128,
    pub haircut_denominator: u128,
    pub conservation: bool,
    pub current_slot: u64,
    pub last_oracle_price: u64,
    pub last_funding_rate: i64,
    pub side_mode_long: String,
    pub side_mode_short: String,
    pub oi_long_q: u128,
    pub oi_short_q: u128,
    pub lifetime_liquidations: u64,
    pub accounts: Vec<AccountSnapshot>,
}

impl EngineSnapshot {
    pub fn haircut_ratio_f64(&self) -> f64 {
        if self.haircut_denominator == 0 {
            1.0
        } else {
            self.haircut_numerator as f64 / self.haircut_denominator as f64
        }
    }
}

/// Serializable snapshot of a single account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub name: String,
    pub index: u16,
    pub capital: u128,
    pub pnl: i128,
    pub reserved_pnl: u128,
    pub effective_position_q: i128,
    pub effective_matured_pnl: u128,
    pub equity_maint: i128,
    pub equity_init: i128,
    pub above_maintenance_margin: bool,
    pub above_initial_margin: bool,
    pub notional: u128,
    pub fee_credits: i128,
}

/// Serde-friendly risk parameter configuration with sensible defaults.
///
/// All BPS values are in basis points (1 bps = 0.01%).
/// Converts to [`RiskParams`] via [`to_risk_params()`](ParamsConfig::to_risk_params).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsConfig {
    #[serde(default = "default_warmup")]
    pub warmup_period_slots: u64,
    #[serde(default = "default_mm_bps")]
    pub maintenance_margin_bps: u64,
    #[serde(default = "default_im_bps")]
    pub initial_margin_bps: u64,
    #[serde(default)]
    pub trading_fee_bps: u64,
    #[serde(default = "default_max_accounts")]
    pub max_accounts: u64,
    #[serde(default)]
    pub new_account_fee: u64,
    #[serde(default)]
    pub maintenance_fee_per_slot: u64,
    #[serde(default = "default_crank_staleness")]
    pub max_crank_staleness_slots: u64,
    #[serde(default)]
    pub liquidation_fee_bps: u64,
    #[serde(default)]
    pub liquidation_fee_cap: u64,
    #[serde(default)]
    pub min_liquidation_abs: u64,
    #[serde(default = "default_min_deposit")]
    pub min_initial_deposit: u64,
    #[serde(default = "default_mm_req")]
    pub min_nonzero_mm_req: u64,
    #[serde(default = "default_im_req")]
    pub min_nonzero_im_req: u64,
    #[serde(default)]
    pub insurance_floor: u64,
}

fn default_warmup() -> u64 {
    100
}
fn default_mm_bps() -> u64 {
    500
}
fn default_im_bps() -> u64 {
    1000
}
fn default_max_accounts() -> u64 {
    64
}
fn default_crank_staleness() -> u64 {
    u64::MAX
}
fn default_mm_req() -> u64 {
    1
}
fn default_im_req() -> u64 {
    2
}
fn default_min_deposit() -> u64 {
    2
}
impl ParamsConfig {
    pub fn to_risk_params(&self) -> RiskParams {
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

impl Default for ParamsConfig {
    fn default() -> Self {
        Self {
            warmup_period_slots: default_warmup(),
            maintenance_margin_bps: default_mm_bps(),
            initial_margin_bps: default_im_bps(),
            trading_fee_bps: 0,
            max_accounts: default_max_accounts(),
            new_account_fee: 0,
            maintenance_fee_per_slot: 0,
            max_crank_staleness_slots: default_crank_staleness(),
            liquidation_fee_bps: 0,
            liquidation_fee_cap: 0,
            min_liquidation_abs: 0,
            min_initial_deposit: default_min_deposit(),
            min_nonzero_mm_req: default_mm_req(),
            min_nonzero_im_req: default_im_req(),
            insurance_floor: 0,
        }
    }
}

/// Errors from the [`NamedEngine`] wrapper layer.
#[derive(Debug)]
pub enum EngineError {
    UnknownAccount(String),
    AccountLimitReached(usize),
    InvalidAccountName(String),
    InvalidAmount,
    InvalidOraclePrice(u64),
    Risk(RiskError),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAccount(name) => write!(f, "unknown account: {name}"),
            Self::AccountLimitReached(max) => write!(f, "account limit reached ({max})"),
            Self::InvalidAccountName(name) => write!(f, "invalid account name: {name:?}"),
            Self::InvalidAmount => write!(f, "amount must be non-zero"),
            Self::InvalidOraclePrice(p) => write!(f, "invalid oracle price: {p}"),
            Self::Risk(e) => write!(f, "risk engine error: {e:?}"),
        }
    }
}

impl std::error::Error for EngineError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_engine() -> NamedEngine {
        let params = ParamsConfig::default().to_risk_params();
        NamedEngine::new(params, 0, 1000)
    }

    #[test]
    fn deposit_withdraw_roundtrip() {
        let mut eng = default_engine();
        eng.deposit("alice", 100_000).unwrap();
        let snap = eng.snapshot();
        assert_eq!(snap.vault, 100_000);
        assert_eq!(snap.accounts[0].capital, 100_000);

        eng.withdraw("alice", 50_000).unwrap();
        let snap = eng.snapshot();
        assert_eq!(snap.accounts[0].capital, 50_000);
    }

    #[test]
    fn account_overflow_rejected() {
        let params = ParamsConfig {
            max_accounts: 2,
            ..Default::default()
        };
        let risk = params.to_risk_params();
        let mut eng = NamedEngine::new(risk, 0, 1000);

        eng.deposit("a", 100).unwrap();
        eng.deposit("b", 100).unwrap();

        let err = eng.deposit("c", 100).unwrap_err();
        assert!(matches!(err, EngineError::AccountLimitReached(_)));
    }

    #[test]
    fn name_resolution() {
        let mut eng = default_engine();
        assert!(eng.resolve("alice").is_err());

        eng.deposit("alice", 1000).unwrap();
        assert_eq!(eng.resolve("alice").unwrap(), 0);
    }

    #[test]
    fn zero_amount_rejected() {
        let mut eng = default_engine();
        let err = eng.deposit("alice", 0).unwrap_err();
        assert!(matches!(err, EngineError::InvalidAmount));
    }

    #[test]
    fn trade_basic() {
        let mut eng = default_engine();
        eng.deposit("alice", 100_000).unwrap();
        eng.deposit("bob", 100_000).unwrap();
        eng.crank(1000, 1).unwrap();
        eng.trade("alice", "bob", 10 * POS_SCALE as i128, 1000)
            .unwrap();

        let snap = eng.snapshot();
        assert_eq!(snap.vault, 200_000);
        assert!(snap.conservation);
    }

    #[test]
    fn snapshot_serialization_roundtrip() {
        let mut eng = default_engine();
        eng.deposit("alice", 50_000).unwrap();
        let snap = eng.snapshot();

        let json = serde_json::to_string(&snap).unwrap();
        let deser: EngineSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.vault, snap.vault);
        assert_eq!(deser.accounts.len(), 1);
        assert_eq!(deser.accounts[0].name, "alice");
    }

    #[test]
    fn empty_name_rejected() {
        let mut eng = default_engine();
        let err = eng.deposit("", 100).unwrap_err();
        assert!(matches!(err, EngineError::InvalidAccountName(_)));
    }

    #[test]
    fn duplicate_account_idempotent() {
        let mut eng = default_engine();
        eng.deposit("alice", 1000).unwrap();
        eng.deposit("alice", 2000).unwrap();
        assert_eq!(eng.resolve("alice").unwrap(), 0);
        let snap = eng.snapshot();
        assert_eq!(snap.accounts.len(), 1);
        assert_eq!(snap.accounts[0].capital, 3000);
    }

    #[test]
    fn invalid_oracle_price_rejected() {
        let mut eng = default_engine();
        assert!(eng.set_oracle(0).is_err());
    }

    #[test]
    fn oracle_price_above_max_rejected() {
        let mut eng = default_engine();
        assert!(eng.set_oracle(MAX_ORACLE_PRICE + 1).is_err());
        assert!(eng.set_oracle(MAX_ORACLE_PRICE).is_ok());
    }

    #[test]
    fn withdraw_unknown_account_rejected() {
        let mut eng = default_engine();
        let err = eng.withdraw("ghost", 100).unwrap_err();
        assert!(matches!(err, EngineError::UnknownAccount(_)));
    }

    #[test]
    fn withdraw_zero_rejected() {
        let mut eng = default_engine();
        eng.deposit("alice", 1000).unwrap();
        let err = eng.withdraw("alice", 0).unwrap_err();
        assert!(matches!(err, EngineError::InvalidAmount));
    }

    #[test]
    fn long_account_name_rejected() {
        let mut eng = default_engine();
        let name = "a".repeat(65);
        let err = eng.deposit(&name, 100).unwrap_err();
        assert!(matches!(err, EngineError::InvalidAccountName(_)));
    }

    #[test]
    fn max_length_account_name_accepted() {
        let mut eng = default_engine();
        let name = "a".repeat(64);
        eng.deposit(&name, 100).unwrap();
        assert_eq!(eng.resolve(&name).unwrap(), 0);
    }

    #[test]
    fn trade_unknown_account_rejected() {
        let mut eng = default_engine();
        eng.deposit("alice", 100_000).unwrap();
        let err = eng
            .trade("alice", "ghost", 10 * POS_SCALE as i128, 1000)
            .unwrap_err();
        assert!(matches!(err, EngineError::UnknownAccount(_)));
    }

    #[test]
    fn liquidate_unknown_account_rejected() {
        let mut eng = default_engine();
        let err = eng.liquidate("ghost").unwrap_err();
        assert!(matches!(err, EngineError::UnknownAccount(_)));
    }

    #[test]
    fn settle_unknown_account_rejected() {
        let mut eng = default_engine();
        let err = eng.settle("ghost").unwrap_err();
        assert!(matches!(err, EngineError::UnknownAccount(_)));
    }

    #[test]
    fn set_slot_and_funding_rate() {
        let mut eng = default_engine();
        eng.set_slot(42);
        assert_eq!(eng.current_slot, 42);
        eng.set_funding_rate(-100);
        assert_eq!(eng.last_funding_rate, -100);
    }

    #[test]
    fn crank_updates_oracle_and_slot() {
        let mut eng = default_engine();
        eng.deposit("alice", 100_000).unwrap();
        eng.crank(2000, 50).unwrap();
        assert_eq!(eng.last_oracle_price, 2000);
        assert_eq!(eng.current_slot, 50);
    }

    #[test]
    fn snapshot_empty_engine() {
        let eng = default_engine();
        let snap = eng.snapshot();
        assert_eq!(snap.vault, 0);
        assert_eq!(snap.accounts.len(), 0);
        assert!(snap.conservation);
    }

    #[test]
    fn error_display() {
        let e = EngineError::UnknownAccount("bob".to_string());
        assert_eq!(e.to_string(), "unknown account: bob");

        let e = EngineError::AccountLimitReached(64);
        assert_eq!(e.to_string(), "account limit reached (64)");

        let e = EngineError::InvalidAccountName("".to_string());
        assert_eq!(e.to_string(), "invalid account name: \"\"");

        let e = EngineError::InvalidAmount;
        assert_eq!(e.to_string(), "amount must be non-zero");

        let e = EngineError::InvalidOraclePrice(0);
        assert_eq!(e.to_string(), "invalid oracle price: 0");
    }

    #[test]
    fn params_config_default_roundtrip() {
        let params = ParamsConfig::default();
        let json = serde_json::to_string(&params).unwrap();
        let deser: ParamsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.maintenance_margin_bps, params.maintenance_margin_bps);
        assert_eq!(deser.initial_margin_bps, params.initial_margin_bps);
        assert_eq!(deser.warmup_period_slots, params.warmup_period_slots);
    }

    #[test]
    fn params_config_from_empty_json() {
        let deser: ParamsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(deser.maintenance_margin_bps, default_mm_bps());
        assert_eq!(deser.initial_margin_bps, default_im_bps());
        assert_eq!(deser.warmup_period_slots, default_warmup());
        assert_eq!(deser.max_accounts, default_max_accounts());
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    fn bounded_engine() -> NamedEngine {
        let params = ParamsConfig {
            max_accounts: MAX_ACCOUNTS as u64,
            ..Default::default()
        }
        .to_risk_params();
        NamedEngine::new(params, 0, 1000)
    }

    // --- Fast proofs (< 10s): input validation and pure logic ---

    #[kani::proof]
    fn prove_deposit_zero_always_fails() {
        let mut eng = bounded_engine();
        assert!(eng.deposit("alice", 0).is_err());
    }

    #[kani::proof]
    fn prove_withdraw_unknown_account_fails() {
        let eng = bounded_engine();
        assert!(eng.resolve("nonexistent").is_err());
    }

    #[kani::proof]
    fn prove_set_oracle_bounds() {
        let mut eng = bounded_engine();
        let price: u64 = kani::any();
        let result = eng.set_oracle(price);
        if price == 0 || price > MAX_ORACLE_PRICE {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
            assert_eq!(eng.last_oracle_price, price);
        }
    }

    #[kani::proof]
    fn prove_ensure_account_name_validation() {
        let mut eng = bounded_engine();
        assert!(eng.deposit("", 100).is_err());
    }

    #[kani::proof]
    fn prove_haircut_ratio_no_div_by_zero() {
        let snap = EngineSnapshot {
            vault: 0,
            insurance_fund: 0,
            haircut_numerator: kani::any(),
            haircut_denominator: 0,
            conservation: true,
            current_slot: 0,
            last_oracle_price: 1000,
            last_funding_rate: 0,
            side_mode_long: String::new(),
            side_mode_short: String::new(),
            oi_long_q: 0,
            oi_short_q: 0,
            lifetime_liquidations: 0,
            accounts: vec![],
        };
        let h = snap.haircut_ratio_f64();
        assert!(h == 1.0);
    }

    #[kani::proof]
    fn prove_haircut_ratio_bounded() {
        let num: u128 = kani::any();
        let den: u128 = kani::any();
        kani::assume(den > 0);
        kani::assume(num <= den);

        let snap = EngineSnapshot {
            vault: 0,
            insurance_fund: 0,
            haircut_numerator: num,
            haircut_denominator: den,
            conservation: true,
            current_slot: 0,
            last_oracle_price: 1000,
            last_funding_rate: 0,
            side_mode_long: String::new(),
            side_mode_short: String::new(),
            oi_long_q: 0,
            oi_short_q: 0,
            lifetime_liquidations: 0,
            accounts: vec![],
        };
        let h = snap.haircut_ratio_f64();
        assert!(h >= 0.0 && h <= 1.0);
    }

    #[kani::proof]
    fn prove_set_slot_always_succeeds() {
        let mut eng = bounded_engine();
        let slot: u64 = kani::any();
        eng.set_slot(slot);
        assert_eq!(eng.current_slot, slot);
    }

    #[kani::proof]
    fn prove_set_funding_rate_always_succeeds() {
        let mut eng = bounded_engine();
        let rate: i64 = kani::any();
        eng.set_funding_rate(rate);
        assert_eq!(eng.last_funding_rate, rate);
    }

    #[kani::proof]
    fn prove_params_to_risk_params_no_panic() {
        let bps: u64 = kani::any();
        kani::assume(bps <= 10_000);
        let params = ParamsConfig {
            maintenance_margin_bps: bps,
            initial_margin_bps: bps,
            max_accounts: MAX_ACCOUNTS as u64,
            ..Default::default()
        };
        let _rp = params.to_risk_params();
    }

    // --- Slow proofs (CI-only, > 30s): require BTreeMap operations ---

    #[kani::proof]
    #[kani::unwind(6)]
    fn prove_deposit_never_panics() {
        let mut eng = bounded_engine();
        let amount: u128 = kani::any();
        kani::assume(amount <= MAX_VAULT_TVL);
        let _ = eng.deposit("alice", amount);
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn prove_deposit_increases_vault() {
        let mut eng = bounded_engine();
        let amount: u128 = kani::any();
        kani::assume(amount > 0 && amount <= 1_000_000_000);

        let vault_before = eng.engine.vault.get();
        let result = eng.deposit("alice", amount);
        if result.is_ok() {
            let vault_after = eng.engine.vault.get();
            assert!(vault_after >= vault_before + amount);
        }
    }
}
