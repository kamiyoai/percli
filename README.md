# percli

CLI toolkit for the [Percolator](https://github.com/aeyakovenko/percolator) risk engine — simulate, test, and operate perp markets on Solana.

> **This software has not been audited by a third-party security firm. Use at your own risk. The authors accept no liability for loss of funds.**

## Install

**Pre-built binary** (Linux, macOS):

```bash
curl -fsSL https://raw.githubusercontent.com/kamiyoai/percli/master/install.sh | bash
```

**From source** (requires Rust):

```bash
cargo install percli
```

**With on-chain Solana commands:**

```bash
cargo install percli --features chain
```

**With live Pyth oracle feeds:**

```bash
cargo install percli --features pyth
```

**Pre-built binaries** for all platforms are also available on the [releases page](https://github.com/kamiyoai/percli/releases).

## Quick Start

No setup required — generate a scenario, run it, and inspect the output:

```bash
# Generate a starter scenario
percli init --template basic --output demo.toml

# Run the simulation
percli sim demo.toml

# Run with verbose deltas
percli sim demo.toml --verbose

# Output as JSON (for scripts and pipelines)
percli sim demo.toml --format json
```

## Commands

| Command | Description | Example |
|---------|-------------|---------|
| `sim` | Run a TOML scenario file | `percli sim scenario.toml --verbose` |
| `step` | Execute a single operation on saved state | `percli step --state engine.json deposit alice 100000` |
| `query` | Read-only queries on engine state | `percli query --state engine.json vault` |
| `inspect` | Validate a scenario without running it | `percli inspect scenario.toml` |
| `init` | Generate a scenario template | `percli init --template liquidation` |
| `agent` | Run an external process as a trading agent | `percli agent run --config agent.toml` |
| `chain` | Interact with on-chain Solana program | `percli chain deploy` |
| `keeper` | Auto-crank and auto-liquidate on-chain | `percli keeper --interval 10` |
| `completions` | Generate shell completions | `percli completions zsh` |

### Simulation Options

```bash
# Step-by-step — print state after each operation
percli sim scenario.toml --step-by-step

# Override parameters without editing the file
percli sim scenario.toml --override maintenance_margin_bps=300

# Disable conservation checks
percli sim scenario.toml --no-check-conservation
```

### Interactive State Management

Build up engine state incrementally with `step` and inspect it with `query`:

```bash
# Initialize state with deposits
percli step --state engine.json deposit alice 100000
percli step --state engine.json deposit bob 100000

# Update oracle and execute a trade
percli step --state engine.json crank --oracle 1000 --slot 1
percli step --state engine.json trade alice bob 50 --price 1000

# Query the result
percli query --state engine.json vault
percli query --state engine.json equity --account alice
percli query --state engine.json summary --format json
```

Available query metrics: `summary`, `vault`, `haircut`, `conservation`, `accounts`, `equity`, `margin`, `position`.

## Scenarios

Scenarios are TOML files that define market parameters and a sequence of operations:

```toml
[meta]
name = "Basic Two-Party Trade"

[params]
maintenance_margin_bps = 500    # 5%
initial_margin_bps = 1000       # 10%

[market]
initial_oracle_price = 1000

[[steps]]
action = "deposit"
account = "alice"
amount = 100_000

[[steps]]
action = "trade"
long = "alice"
short = "bob"
size = 50
price = 1000

[[steps]]
action = "assert"
condition = "conservation"
```

### Bundled Scenarios

| Scenario | What it tests |
|----------|---------------|
| `basic-trade.toml` | Two-party trade, 10% price move, equity changes |
| `liquidation-cascade.toml` | High-leverage position, 50% crash, cascading liquidation |
| `haircut-stress.toml` | Multiple traders, extreme price move, haircut activation |
| `insurance-depletion.toml` | Catastrophic loss, insurance absorption, conservation proof |
| `funding-drift.toml` | Funding rate impact over 500 slots with steady price |

Run all bundled scenarios:

```bash
for f in scenarios/*.toml; do percli sim "$f"; done
```

## Agent Mode

Spawn any process (Python, Node, Bash) as a trading agent. percli feeds it NDJSON tick data with full engine snapshots; the agent responds with actions.

```bash
# Generate a starter agent config
percli agent init --output agent.toml

# Run the agent
percli agent run --config agent.toml

# Dry run — validate config without spawning the process
percli agent run --config agent.toml --dry-run
```

### Example: Liquidation Bot (Python)

```python
import sys, json

for line in sys.stdin:
    msg = json.loads(line)
    if msg["type"] == "done":
        break
    if msg["type"] != "tick":
        continue

    actions = []
    for acct in msg["snapshot"]["accounts"]:
        if not acct["above_maintenance_margin"] and acct["effective_position_q"] != 0:
            actions.append({"op": "liquidate", "account": acct["name"]})

    print(json.dumps({"actions": actions}), flush=True)
```

### Protocol

Agents communicate via NDJSON on stdin/stdout:

1. **Init** — `{"type": "init", "params": {...}, "accounts": [...], "snapshot": {...}}`
2. **Tick** (per price update) — `{"type": "tick", "tick": 1, "oracle_price": 1050, "snapshot": {...}}`
3. **Response** (agent → percli) — `{"actions": [{"op": "liquidate", "account": "alice"}, ...]}`
4. **Done** — `{"type": "done", "ticks": 100, "elapsed_s": 1.2}`

Available actions: `deposit`, `withdraw`, `trade`, `liquidate`, `settle`, `noop`.

Price feeds can be inline TOML arrays, CSV files, stdin, or live Pyth oracle streams (requires `--features pyth`).

### Pyth Live Feed

Stream real-time prices from Pyth Network into agent mode:

```toml
[feed]
type = "pyth"
rpc_url = "https://api.mainnet-beta.solana.com"
feed_id = "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG"  # SOL/USD
poll_ms = 2000
max_ticks = 500
```

See [`examples/agent-pyth.toml`](examples/agent-pyth.toml) for a complete config.

## On-Chain (Solana)

The `chain` feature adds commands for interacting with a deployed Percolator market on Solana.

Deposits and withdrawals move real SPL tokens (e.g. USDC) between user wallets and a PDA-controlled vault. The crank instruction reads oracle prices directly from a [Pyth Network](https://pyth.network/) price feed account on-chain.

```bash
# Deploy a new market
percli chain deploy

# Deposit (SPL token transfer from user to vault)
percli chain deposit --idx 0 --amount 100000 \
  --mint <MINT_PUBKEY> --token-account <USER_ATA>

# Trade between two accounts
percli chain trade --a 0 --b 1 --size 100 --price 1000

# Crank — reads price from Pyth oracle on-chain
percli chain crank --oracle <PYTH_FEED_PUBKEY>

# Withdraw (SPL token transfer from vault to user, margin-checked)
percli chain withdraw --idx 0 --amount 50000 \
  --mint <MINT_PUBKEY> --token-account <USER_ATA>

# Liquidate and settle
percli chain liquidate --idx 0
percli chain settle --idx 1

# Query on-chain state
percli chain query market
percli chain query 0   # query account at index 0
```

Global options: `--rpc <url>`, `--keypair <path>`, `--program <pubkey>`.

The on-chain program (`percli-program`) is an Anchor 1.0 program that wraps the Percolator engine in a Solana PDA. See [`Anchor.toml`](Anchor.toml) for deployment config.

## Keeper Bot

The `keeper` command watches an on-chain market, cranks Pyth oracle updates, and liquidates undercollateralized accounts:

```bash
# Keeper with Pyth oracle feed (required)
percli keeper --rpc devnet --pyth-feed <PYTH_FEED_PUBKEY> --interval 10
```

The keeper loops: read market state, submit crank tx (which reads the Pyth price on-chain), liquidate anyone below maintenance margin, sleep, repeat.

Requires `--features chain`.

## Playground

Try percli in your browser at [kamiyoai.github.io/percli](https://kamiyoai.github.io/percli) — no install required. Edit TOML scenarios and see simulation results instantly via WebAssembly.

## What is Percolator?

[Percolator](https://github.com/aeyakovenko/percolator) is a risk engine for perpetual futures that replaces ADL queues with two deterministic mechanisms:

**H (haircut ratio)** — When the vault is stressed, every profitable account sees the same pro-rata scaling on withdrawable profit. No queue priority, no first-come advantage. Capital deposits are always protected.

**A/K (side indices)** — When a leveraged account goes bankrupt, the opposing side absorbs the residual through global position scaling (A) and PnL socialization (K). No account is singled out. Settlement is O(1) per account.

Together: no user can withdraw more than exists, no user is singled out for forced closure, and markets always self-heal through a deterministic three-phase reset — no admin intervention, no governance votes.

See [Tarun Chitra, *Autodeleveraging: Impossibilities and Optimization*](https://arxiv.org/abs/2512.01112) for the theoretical foundation.

## Architecture

```
kamiyoai/percli (workspace)
├── src/              # percolator — upstream risk engine (no-std, formally verified)
├── crates/
│   ├── percli-core/  # engine wrapper, scenario runner, agent protocol
│   ├── percli/       # CLI binary (sim, step, query, agent, chain)
│   ├── percli-chain/ # Solana RPC client commands
│   ├── percli-program/ # Anchor on-chain program
│   └── percli-wasm/  # WebAssembly build
├── web/              # browser playground (GitHub Pages)
├── scenarios/        # bundled TOML test scenarios
├── examples/         # agent examples (Python, Bash)
├── tests/            # upstream Kani formal verification proofs
└── scripts/          # development utilities
```

## Security

The core risk engine (`percolator`) is formally verified with [Kani](https://model-checking.github.io/kani/) proofs and continuously fuzz-tested with [proptest](https://crates.io/crates/proptest). All arithmetic uses checked operations; `#![forbid(unsafe_code)]` is enforced in the engine crate.

The on-chain Anchor program validates:
- **Account ownership** — market accounts must be owned by the program
- **Oracle authenticity** — price feeds must be owned by the Pyth v2 program
- **Discriminator + size checks** — all market accounts are validated before access
- **SPL token constraints** — mint, owner, and vault PDA seeds are verified by Anchor
- **Checked price conversion** — Pyth exponent handling uses checked arithmetic with bounded exponent range

**This software has not been audited by a third-party security firm.** If you discover a vulnerability, please report it privately via GitHub Security Advisories rather than opening a public issue.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and PR guidelines.

## License

Apache-2.0 OR MIT — see [LICENSE](LICENSE).
