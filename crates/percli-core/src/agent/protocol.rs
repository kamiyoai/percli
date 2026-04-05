use serde::{Deserialize, Serialize};

use crate::{EngineSnapshot, ParamsConfig};

/// Sent once at the start of an agent session.
#[derive(Debug, Clone, Serialize)]
pub struct InitMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub params: ParamsConfig,
    pub accounts: Vec<String>,
    pub snapshot: EngineSnapshot,
}

impl InitMessage {
    pub fn new(params: ParamsConfig, accounts: Vec<String>, snapshot: EngineSnapshot) -> Self {
        Self {
            msg_type: "init",
            params,
            accounts,
            snapshot,
        }
    }
}

/// Sent each tick with current market state and engine snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct TickMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub tick: u64,
    pub oracle_price: u64,
    pub slot: u64,
    pub snapshot: EngineSnapshot,
}

impl TickMessage {
    pub fn new(tick: u64, oracle_price: u64, slot: u64, snapshot: EngineSnapshot) -> Self {
        Self {
            msg_type: "tick",
            tick,
            oracle_price,
            slot,
            snapshot,
        }
    }
}

/// Sent when the feed is exhausted or the run ends.
#[derive(Debug, Clone, Serialize)]
pub struct DoneMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub ticks: u64,
    pub elapsed_s: f64,
}

impl DoneMessage {
    pub fn new(ticks: u64, elapsed_s: f64) -> Self {
        Self {
            msg_type: "done",
            ticks,
            elapsed_s,
        }
    }
}

/// Response from the agent process.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Response(AgentResponse),
    Shutdown(ShutdownMessage),
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentResponse {
    pub actions: Vec<AgentAction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShutdownMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
}

impl ShutdownMessage {
    pub fn is_shutdown(&self) -> bool {
        self.msg_type == "shutdown"
    }
}

/// A single action the agent wants to execute.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentAction {
    Deposit {
        account: String,
        amount: u64,
    },
    Withdraw {
        account: String,
        amount: u64,
    },
    Trade {
        long: String,
        short: String,
        size: i64,
        price: u64,
    },
    Liquidate {
        account: String,
    },
    Settle {
        account: String,
    },
    Noop,
}
