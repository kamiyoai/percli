use anyhow::Result;
use clap::Subcommand;
use percli_chain::commands::query::QueryTarget;
use percli_chain::ChainConfig;
use solana_sdk::pubkey::Pubkey;

#[derive(Subcommand)]
pub enum ChainCommand {
    /// Deploy a new market (initialize_market)
    Deploy {
        /// SPL token mint address (e.g. USDC)
        #[arg(long)]
        mint: Pubkey,
        /// Pyth price feed account address
        #[arg(long)]
        oracle: Pubkey,
        /// Initial oracle price
        #[arg(long, default_value = "1000")]
        init_price: u64,
    },
    /// Deposit to an account slot
    Deposit {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Amount to deposit (token base units)
        #[arg(long)]
        amount: u64,
        /// SPL token mint address
        #[arg(long)]
        mint: Pubkey,
        /// User's token account address
        #[arg(long)]
        token_account: Pubkey,
    },
    /// Withdraw from an account slot
    Withdraw {
        /// Account slot index
        #[arg(long)]
        idx: u16,
        /// Amount to withdraw (token base units)
        #[arg(long)]
        amount: u64,
        /// Funding rate
        #[arg(long, default_value = "0")]
        funding_rate: i64,
        /// SPL token mint address
        #[arg(long)]
        mint: Pubkey,
        /// User's token account address
        #[arg(long)]
        token_account: Pubkey,
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
    /// Run a keeper crank (read oracle price from Pyth)
    Crank {
        /// Pyth price feed account address
        #[arg(long)]
        oracle: Pubkey,
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
        ChainCommand::Deploy { mint, oracle, init_price } => {
            percli_chain::commands::deploy::run(&config, &mint, &oracle, init_price)
        }
        ChainCommand::Deposit { idx, amount, mint, token_account } => {
            percli_chain::commands::deposit::run(&config, idx, amount, &mint, &token_account)
        }
        ChainCommand::Withdraw {
            idx,
            amount,
            funding_rate,
            mint,
            token_account,
        } => percli_chain::commands::withdraw::run(&config, idx, amount, funding_rate, &mint, &token_account),
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
        } => percli_chain::commands::crank::run(&config, &oracle, funding_rate),
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
