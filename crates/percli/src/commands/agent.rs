use anyhow::{bail, Context, Result};
use percli_core::agent::feed::FeedConfig;
use percli_core::agent::protocol::{
    AgentAction, AgentMessage, DoneMessage, InitMessage, TickMessage,
};
use percli_core::{NamedEngine, ParamsConfig, POS_SCALE};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::format::status;

#[derive(Debug, Deserialize)]
struct AgentConfig {
    #[serde(default)]
    meta: AgentMeta,
    agent: AgentProcess,
    #[serde(default)]
    params: ParamsConfig,
    #[serde(default)]
    market: MarketConfig,
    #[serde(default)]
    feed: FeedConfig,
    #[serde(default)]
    budget: BudgetConfig,
    #[serde(default)]
    setup: Vec<SetupStep>,
}

#[derive(Debug, Default, Deserialize)]
struct AgentMeta {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct AgentProcess {
    command: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 {
    5000
}

#[derive(Debug, Deserialize)]
struct MarketConfig {
    #[serde(default = "default_oracle_price")]
    initial_oracle_price: u64,
    #[serde(default)]
    initial_slot: u64,
}

fn default_oracle_price() -> u64 {
    1000
}

impl Default for MarketConfig {
    fn default() -> Self {
        Self {
            initial_oracle_price: default_oracle_price(),
            initial_slot: 0,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct BudgetConfig {
    #[serde(default = "default_max_ticks")]
    max_ticks: u64,
    #[serde(default)]
    #[allow(dead_code)]
    accounts: Vec<String>,
}

fn default_max_ticks() -> u64 {
    10_000
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SetupStep {
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
    Crank {
        oracle_price: u64,
        slot: u64,
    },
}

pub fn run(config_path: &Path, verbose_ticks: bool, dry_run: bool) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read config: {}", config_path.display()))?;
    let config: AgentConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config: {}", config_path.display()))?;

    let name = if config.meta.name.is_empty() {
        config_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "agent".to_string())
    } else {
        config.meta.name.clone()
    };

    let risk_params = config.params.to_risk_params();
    let mut engine = NamedEngine::new(
        risk_params,
        config.market.initial_slot,
        config.market.initial_oracle_price,
    );

    // Run setup steps
    for (i, step) in config.setup.iter().enumerate() {
        run_setup_step(&mut engine, step)
            .with_context(|| format!("setup step {} failed", i + 1))?;
    }

    let setup_snap = engine.snapshot();
    let account_names: Vec<String> = engine.accounts.keys().cloned().collect();

    status::status(
        "Running",
        &format!("{} ({} accounts)", name, account_names.len()),
    );

    if dry_run {
        status::status_cyan("Dry run", "skipping agent process, validating config only");
        let feed_iter = config.feed.into_tick_iter()?;
        let tick_count = feed_iter.count();
        status::status_cyan("Feed", &format!("{} ticks available", tick_count));
        return Ok(());
    }

    // Spawn agent process
    if config.agent.command.is_empty() {
        bail!("agent.command must not be empty");
    }
    let mut child = Command::new(&config.agent.command[0])
        .args(&config.agent.command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn agent: {}", config.agent.command.join(" ")))?;

    let mut agent_stdin = child.stdin.take().expect("stdin was piped");
    let agent_stdout = child.stdout.take().expect("stdout was piped");

    // Set up reader thread for agent stdout with timeout support
    let (tx, rx) = mpsc::channel::<String>();
    let reader_thread = std::thread::spawn(move || {
        let reader = BufReader::new(agent_stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let timeout = Duration::from_millis(config.agent.timeout_ms);
    let start = Instant::now();

    // Send init message
    let init_msg = InitMessage::new(config.params.clone(), account_names.clone(), setup_snap);
    writeln!(agent_stdin, "{}", serde_json::to_string(&init_msg)?)?;
    agent_stdin.flush()?;

    // Tick loop
    let feed_iter = config.feed.into_tick_iter()?;
    let mut tick_num: u64 = 0;
    let mut total_actions: u64 = 0;
    let mut _shutdown = false;
    let max_ticks = config.budget.max_ticks;

    for price_tick in feed_iter {
        if tick_num >= max_ticks {
            status::status_warn(
                "Budget",
                &format!("max_ticks ({}) reached, stopping", max_ticks),
            );
            break;
        }

        // Crank engine with new price
        engine.crank(price_tick.oracle, price_tick.slot)?;
        tick_num += 1;

        let snapshot = engine.snapshot();
        let tick_msg = TickMessage::new(tick_num, price_tick.oracle, price_tick.slot, snapshot);

        if verbose_ticks {
            status::status_cyan(
                &format!("Tick {}", tick_num),
                &format!("oracle={} slot={}", price_tick.oracle, price_tick.slot),
            );
        }

        // Send tick to agent
        writeln!(agent_stdin, "{}", serde_json::to_string(&tick_msg)?)?;
        agent_stdin.flush()?;

        // Read response with timeout
        let response_line = match rx.recv_timeout(timeout) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                status::error(&format!(
                    "agent timed out after {}ms on tick {}",
                    config.agent.timeout_ms, tick_num
                ));
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                status::error("agent process exited unexpectedly");
                break;
            }
        };

        // Try parse as shutdown first, then as response
        let msg: AgentMessage = serde_json::from_str(&response_line).with_context(|| {
            format!(
                "failed to parse agent response on tick {}: {}",
                tick_num, response_line
            )
        })?;

        match msg {
            AgentMessage::Shutdown(s) if s.is_shutdown() => {
                status::status_cyan("Shutdown", "agent requested early termination");
                _shutdown = true;
                break;
            }
            AgentMessage::Shutdown(_) => {
                // Unknown message type, treat as empty response
            }
            AgentMessage::Response(resp) => {
                for action in &resp.actions {
                    if let Err(e) = execute_action(&mut engine, action) {
                        status::status_warn(
                            "Action",
                            &format!("tick {}: {} ({})", tick_num, describe_action(action), e),
                        );
                    } else {
                        total_actions += 1;
                        if verbose_ticks {
                            status::status(
                                "Action",
                                &format!("tick {}: {}", tick_num, describe_action(action)),
                            );
                        }
                    }
                }
            }
        }
    }

    // Send done message
    let done_msg = DoneMessage::new(tick_num, start.elapsed().as_secs_f64());
    let _ = writeln!(agent_stdin, "{}", serde_json::to_string(&done_msg)?);
    let _ = agent_stdin.flush();

    // Wait for agent to exit
    drop(agent_stdin);
    let _ = child.wait();
    let _ = reader_thread.join();

    // Print summary
    let final_snap = engine.snapshot();
    eprintln!();
    status::status("Ticks", &format!("{}", tick_num));
    status::status("Actions", &format!("{}", total_actions));
    status::status("Vault", &format!("{}", final_snap.vault));
    status::status("Insurance", &format!("{}", final_snap.insurance_fund));
    if !final_snap.conservation {
        status::status_warn("Warning", "conservation invariant violated");
    }
    status::finished(start);

    Ok(())
}

fn run_setup_step(engine: &mut NamedEngine, step: &SetupStep) -> Result<()> {
    match step {
        SetupStep::Deposit { account, amount } => {
            engine.deposit(account, *amount as u128)?;
        }
        SetupStep::Withdraw { account, amount } => {
            engine.withdraw(account, *amount as u128)?;
        }
        SetupStep::Trade {
            long,
            short,
            size,
            price,
        } => {
            let size_q = (*size as i128) * (POS_SCALE as i128);
            engine.trade(long, short, size_q, *price)?;
        }
        SetupStep::Crank { oracle_price, slot } => {
            engine.crank(*oracle_price, *slot)?;
        }
    }
    Ok(())
}

fn execute_action(engine: &mut NamedEngine, action: &AgentAction) -> Result<()> {
    match action {
        AgentAction::Deposit { account, amount } => {
            engine.deposit(account, *amount as u128)?;
        }
        AgentAction::Withdraw { account, amount } => {
            engine.withdraw(account, *amount as u128)?;
        }
        AgentAction::Trade {
            long,
            short,
            size,
            price,
        } => {
            let size_q = (*size as i128) * (POS_SCALE as i128);
            engine.trade(long, short, size_q, *price)?;
        }
        AgentAction::Liquidate { account } => {
            engine.liquidate(account)?;
        }
        AgentAction::Settle { account } => {
            engine.settle(account)?;
        }
        AgentAction::Noop => {}
    }
    Ok(())
}

fn describe_action(action: &AgentAction) -> String {
    match action {
        AgentAction::Deposit { account, amount } => format!("deposit {} {}", account, amount),
        AgentAction::Withdraw { account, amount } => format!("withdraw {} {}", account, amount),
        AgentAction::Trade {
            long,
            short,
            size,
            price,
        } => format!("trade {} / {} x {} @ {}", long, short, size, price),
        AgentAction::Liquidate { account } => format!("liquidate {}", account),
        AgentAction::Settle { account } => format!("settle {}", account),
        AgentAction::Noop => "noop".to_string(),
    }
}

pub fn init(output: Option<&Path>) -> Result<()> {
    let template = r#"# percli agent configuration
# Run with: percli agent run --config agent.toml

[meta]
name = "My Agent"

[agent]
# Command to spawn — any executable that reads NDJSON on stdin, writes NDJSON on stdout.
# Examples:
#   command = ["python3", "my_agent.py"]
#   command = ["bash", "noop.sh"]
#   command = ["node", "agent.js"]
command = ["python3", "agent.py"]
timeout_ms = 5000

[params]
maintenance_margin_bps = 500
initial_margin_bps = 1000

[market]
initial_oracle_price = 1000
initial_slot = 0

[feed]
# type = "inline" | "csv" | "stdin"
type = "inline"
prices = [
  { oracle = 1000, slot = 100 },
  { oracle = 1050, slot = 200 },
  { oracle = 1100, slot = 300 },
  { oracle = 1050, slot = 400 },
  { oracle = 900,  slot = 500 },
  { oracle = 850,  slot = 600 },
  { oracle = 800,  slot = 700 },
  { oracle = 900,  slot = 800 },
  { oracle = 1000, slot = 900 },
  { oracle = 1100, slot = 1000 },
]

# For CSV feeds:
# type = "csv"
# path = "prices.csv"    # columns: oracle,slot (no header)

# For live/piped feeds:
# type = "stdin"          # expects NDJSON: {"oracle":1000,"slot":100}

[budget]
max_ticks = 1000
accounts = ["alice", "bob"]

[[setup]]
action = "deposit"
account = "alice"
amount = 100000

[[setup]]
action = "deposit"
account = "bob"
amount = 100000
"#;

    if let Some(path) = output {
        std::fs::write(path, template)?;
        status::status("Writing", &format!("{}", path.display()));
        status::status_cyan(
            "Hint",
            &format!("percli agent run --config {}", path.display()),
        );
    } else {
        print!("{}", template);
    }
    Ok(())
}
