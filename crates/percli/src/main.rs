mod commands;
mod format;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use format::OutputFormat;
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "percli",
    about = "Offline simulator for the Percolator risk engine",
    long_about = "Simulate haircuts, liquidations, margin checks, and conservation proofs\n\
        for the Percolator risk engine \u{2014} no chain required.\n\n\
        Define scenarios in TOML, run simulations, and inspect how the risk\n\
        engine handles stressed markets, undercollateralized accounts, and\n\
        loss socialization.",
    after_help = "See 'percli help <command>' for more on a specific command.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format (table or json). Also settable via PERC_FORMAT env var.
    #[arg(long, short, global = true)]
    format: Option<OutputFormat>,

    /// Color output: auto, always, or never
    #[arg(long, global = true, default_value = "auto")]
    color: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new scenario template
    #[command(after_help = "Examples:\n  \
            percli init\n  \
            percli init --template liquidation\n  \
            percli init --template haircut")]
    Init {
        /// Template to use: basic, liquidation, or haircut
        #[arg(long, default_value = "basic")]
        template: String,
        /// Write to file instead of stdout
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Run a scenario file
    #[command(after_help = "Examples:\n  \
            percli sim scenario.toml\n  \
            percli sim scenario.toml --verbose\n  \
            percli sim scenario.toml --step-by-step\n  \
            percli sim scenario.toml --override maintenance_margin_bps=300")]
    Sim {
        /// Path to scenario TOML file
        path: String,
        /// Print state after each step
        #[arg(long)]
        step_by_step: bool,
        /// Disable conservation checks after each step
        #[arg(long)]
        no_check_conservation: bool,
        /// Verbose output with deltas
        #[arg(long, short)]
        verbose: bool,
        /// Override a parameter: key=value (e.g. maintenance_margin_bps=300)
        #[arg(long = "override", value_name = "KEY=VALUE")]
        overrides: Vec<String>,
    },
    /// Execute a single operation on a saved engine state
    #[command(after_help = "Examples:\n  \
            percli step --state engine.json deposit alice 100000\n  \
            percli step --state engine.json crank --oracle 1100 --slot 200\n  \
            percli step --state engine.json liquidate alice")]
    Step {
        /// Path to engine state JSON file
        #[arg(long)]
        state: String,
        #[command(subcommand)]
        action: StepAction,
    },
    /// Read-only queries on engine state
    #[command(
        after_help = "Metrics: summary, haircut, conservation, vault, equity, margin, position, accounts\n\n\
            Examples:\n  \
            percli query --state engine.json summary\n  \
            percli query --state engine.json haircut\n  \
            percli query --state engine.json equity --account alice"
    )]
    Query {
        /// Path to engine state JSON file
        #[arg(long)]
        state: String,
        /// Metric to query
        metric: String,
        /// Account name (for account-specific queries)
        #[arg(long)]
        account: Option<String>,
    },
    /// Validate a scenario file without running it
    #[command(after_help = "Examples:\n  percli inspect scenario.toml")]
    Inspect {
        /// Path to scenario TOML file
        path: String,
    },
    /// Run an agent against the risk engine
    #[command(after_help = "Examples:\n  \
            percli agent run --config agent.toml\n  \
            percli agent run --config agent.toml --verbose-ticks\n  \
            percli agent init --output agent.toml")]
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
    /// Interact with the on-chain Solana program
    #[cfg(feature = "chain")]
    #[command(after_help = "Examples:\n  \
            percli chain deploy\n  \
            percli chain deposit --idx 0 --amount 1000000\n  \
            percli chain query market\n  \
            percli chain query 0\n  \
            percli chain crank --oracle 1100")]
    Chain {
        /// RPC URL or alias (devnet, mainnet, localhost)
        #[arg(long, global = true)]
        rpc: Option<String>,
        /// Path to keypair JSON file
        #[arg(long, global = true)]
        keypair: Option<String>,
        /// Program ID override
        #[arg(long, global = true)]
        program: Option<String>,
        #[command(subcommand)]
        action: commands::chain::ChainCommand,
    },
    /// Run a keeper that auto-cranks and auto-liquidates on-chain
    #[cfg(feature = "chain")]
    #[command(after_help = "Examples:\n  \
            percli keeper --interval 10\n  \
            percli keeper --rpc devnet --pyth-feed H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG")]
    Keeper {
        /// RPC URL or alias (devnet, mainnet, localhost)
        #[arg(long, global = true)]
        rpc: Option<String>,
        /// Path to keypair JSON file
        #[arg(long, global = true)]
        keypair: Option<String>,
        /// Program ID override
        #[arg(long, global = true)]
        program: Option<String>,
        /// Poll interval in seconds
        #[arg(long, default_value = "10")]
        interval: u64,
        /// Pyth price feed account pubkey (required for oracle reads)
        #[arg(long)]
        pyth_feed: String,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum AgentCommand {
    /// Run an agent process against the tick loop
    #[command(
        after_help = "The agent process receives NDJSON on stdin and writes responses on stdout.\n\n\
            Protocol:\n  \
            percli sends {\"type\":\"init\",...} once, then {\"type\":\"tick\",...} per tick.\n  \
            Agent responds with {\"actions\":[...]} or {\"type\":\"shutdown\"}.\n\n\
            See https://github.com/kamiyoai/percli for protocol docs."
    )]
    Run {
        /// Path to agent config TOML file
        #[arg(long)]
        config: String,
        /// Print each tick and action to stderr
        #[arg(long)]
        verbose_ticks: bool,
        /// Validate config without spawning the agent
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate a starter agent config file
    Init {
        /// Write to file instead of stdout
        #[arg(long, short)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum StepAction {
    /// Deposit to an account
    Deposit { account: String, amount: u128 },
    /// Withdraw from an account
    Withdraw { account: String, amount: u128 },
    /// Execute a trade
    Trade {
        long: String,
        short: String,
        #[arg(long)]
        size: i128,
        #[arg(long)]
        price: u64,
    },
    /// Run a keeper crank
    Crank {
        #[arg(long)]
        oracle: u64,
        #[arg(long)]
        slot: u64,
    },
    /// Liquidate an account
    Liquidate { account: String },
    /// Settle an account's PnL against the vault
    Settle { account: String },
    /// Set oracle price
    SetOracle { price: u64 },
    /// Set current slot
    SetSlot { slot: u64 },
    /// Set funding rate
    SetFundingRate { rate: i64 },
}

fn main() {
    let cli = Cli::parse();

    match cli.color.as_str() {
        "never" => owo_colors::set_override(false),
        "always" => owo_colors::set_override(true),
        _ => {}
    }

    if let Err(e) = run(cli) {
        format::status::error(&format!("{:#}", e));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let fmt = cli.format.unwrap_or_else(OutputFormat::from_env);
    let start = Instant::now();

    match cli.command {
        Commands::Init { template, output } => commands::init::run(&template, output.as_deref()),
        Commands::Sim {
            path,
            step_by_step,
            no_check_conservation,
            verbose,
            overrides,
        } => {
            let result = commands::sim::run(
                &path,
                fmt,
                step_by_step,
                !no_check_conservation,
                verbose,
                &overrides,
            );
            format::status::finished(start);
            result
        }
        Commands::Step { state, action } => commands::step::run(&state, action, fmt),
        Commands::Query {
            state,
            metric,
            account,
        } => commands::query::run(&state, &metric, account.as_deref(), fmt),
        Commands::Inspect { path } => commands::inspect::run(&path),
        Commands::Agent { action } => match action {
            AgentCommand::Run {
                config,
                verbose_ticks,
                dry_run,
            } => commands::agent::run(std::path::Path::new(&config), verbose_ticks, dry_run),
            AgentCommand::Init { output } => {
                commands::agent::init(output.as_deref().map(std::path::Path::new))
            }
        },
        #[cfg(feature = "chain")]
        Commands::Chain {
            rpc,
            keypair,
            program,
            action,
        } => commands::chain::run(
            action,
            rpc.as_deref(),
            keypair.as_deref(),
            program.as_deref(),
        ),
        #[cfg(feature = "chain")]
        Commands::Keeper {
            rpc,
            keypair,
            program,
            interval,
            pyth_feed,
        } => {
            let config = percli_chain::ChainConfig::new(
                rpc.as_deref(),
                keypair.as_deref(),
                program.as_deref(),
            )?;
            commands::keeper::run(&config, interval, &pyth_feed)
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "percli", &mut std::io::stdout());
            Ok(())
        }
    }
}
