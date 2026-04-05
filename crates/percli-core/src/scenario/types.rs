use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub params: crate::ParamsConfig,
    #[serde(default)]
    pub market: MarketConfig,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConfig {
    #[serde(default = "default_oracle")]
    pub initial_oracle_price: u64,
    #[serde(default)]
    pub initial_slot: u64,
}

fn default_oracle() -> u64 {
    1000
}

impl Default for MarketConfig {
    fn default() -> Self {
        Self {
            initial_oracle_price: default_oracle(),
            initial_slot: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    Deposit {
        account: String,
        amount: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    Withdraw {
        account: String,
        amount: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    Trade {
        long: String,
        short: String,
        /// Position size in whole units (auto-scaled by POS_SCALE)
        size: i64,
        price: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    Crank {
        oracle_price: u64,
        slot: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    Liquidate {
        account: String,
        #[serde(default)]
        comment: Option<String>,
    },
    Settle {
        account: String,
        #[serde(default)]
        comment: Option<String>,
    },
    SetOracle {
        oracle_price: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    SetSlot {
        slot: u64,
        #[serde(default)]
        comment: Option<String>,
    },
    SetFundingRate {
        rate: i64,
        #[serde(default)]
        comment: Option<String>,
    },
    Query {
        metric: QueryMetric,
        #[serde(default)]
        comment: Option<String>,
    },
    Assert {
        condition: AssertCondition,
        #[serde(default)]
        comment: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMetric {
    Summary,
    Haircut,
    Vault,
    Accounts,
    Conservation,
    Equity { account: String },
    Margin { account: String },
    Position { account: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertCondition {
    Conservation,
    HaircutBelow { threshold: f64 },
    AboveMaintenanceMargin { account: String },
    BelowMaintenanceMargin { account: String },
    AboveInitialMargin { account: String },
    BelowInitialMargin { account: String },
    VaultAbove { amount: u128 },
    VaultBelow { amount: u128 },
}
