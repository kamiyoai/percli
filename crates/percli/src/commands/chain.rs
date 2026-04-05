use anyhow::Result;
use clap::Subcommand;
use percli_chain::commands::query::QueryTarget;
use percli_chain::ChainConfig;

#[derive(Subcommand)]
pub enum ChainCommand {
    /// Deploy a new market (initialize_market)
    Deploy,
    /// Deposit to an account slot
    Deposit {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Amount to deposit
        #[arg(long)]
        amount: u128,
    },
    /// Withdraw from an account slot
    Withdraw {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Amount to withdraw
        #[arg(long)]
        amount: u128,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Execute a trade between two accounts
    Trade {
        /// Long account index
        #[arg(long = "a")]
        account_a: u16,
        /// Short account index
        #[arg(long = "b")]
        account_b: u16,
        /// Size in base quantity
        #[arg(long)]
        size: i128,
        /// Execution price
        #[arg(long)]
        price: u64,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Run a keeper crank (update oracle price)
    Crank {
        /// Oracle price
        #[arg(long)]
        oracle: u64,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Liquidate an undercollateralized account
    Liquidate {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Settle an account's PnL
    Settle {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Close an account slot
    Close {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
    },
    /// Query on-chain market or account state
    Query {
        /// What to query: "market" or an account index
        target: String,
    },
}

pub fn run(
    cmd: ChainCommand,
    rpc: Option<&str>,
    keypair: Option<&str>,
    program: Option<&str>,
) -> Result<()> {
    let config = ChainConfig::new(rpc, keypair, program)?;

    match cmd {
        ChainCommand::Deploy => percli_chain::commands::deploy::run(&config),
        ChainCommand::Deposit { idx, amount } => {
            percli_chain::commands::deposit::run(&config, idx, amount)
        }
        ChainCommand::Withdraw {
            idx,
            amount,
            funding_rate,
        } => percli_chain::commands::withdraw::run(&config, idx, amount, funding_rate),
        ChainCommand::Trade {
            account_a,
            account_b,
            size,
            price,
            funding_rate,
        } => percli_chain::commands::trade::run(
            &config,
            account_a,
            account_b,
            size,
            price,
            funding_rate,
        ),
        ChainCommand::Crank {
            oracle,
            funding_rate,
        } => percli_chain::commands::crank::run(&config, oracle, funding_rate),
        ChainCommand::Liquidate { idx, funding_rate } => {
            percli_chain::commands::liquidate::run(&config, idx, funding_rate)
        }
        ChainCommand::Settle { idx, funding_rate } => {
            percli_chain::commands::settle::run(&config, idx, funding_rate)
        }
        ChainCommand::Close { idx, funding_rate } => {
            percli_chain::commands::close::run(&config, idx, funding_rate)
        }
        ChainCommand::Query { target } => {
            let qt = if target == "market" {
                QueryTarget::Market
            } else {
                let idx: u16 = target.parse().map_err(|_| {
                    anyhow::anyhow!("Expected 'market' or an account index, got: {target}")
                })?;
                QueryTarget::Account(idx)
            };
            percli_chain::commands::query::run(&config, qt)
        }
    }
}
