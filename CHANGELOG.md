# Changelog

## 0.9.0

### New on-chain instructions
- `top_up_insurance` — permissionless; anyone can add tokens to the insurance fund
- `deposit_fee_credits` — owner-only; deposit tokens to cover accumulated fee debt
- `convert_released_pnl` — permissionless; convert matured PnL after warmup period
- `accrue_market` — permissionless; mark-to-market + funding accrual without touching accounts

### Key rotation
- `update_matcher` — authority can rotate the matcher signing key
- `update_oracle` — authority can rotate the Pyth oracle feed

### Events
- All 6 new instructions emit structured Anchor events (`InsuranceToppedUp`, `FeeCreditsDeposited`, `PnlConverted`, `MarketAccrued`, `MatcherUpdated`, `OracleUpdated`)

### Tests
- 9 new integration tests (29 total) covering all new instructions and auth checks

### Chain CLI
- 6 new `percli chain` subcommands matching the new on-chain instructions

## 0.8.0

### On-chain program
- `withdraw_insurance` — authority can withdraw protocol fees from insurance fund (SPL transfer out, vault PDA signer)
- `reclaim_account` — permissionless dead account slot reclamation
- Event emission for all 10 instructions (`MarketInitialized`, `Deposited`, `Withdrawn`, `Traded`, `Cranked`, `Liquidated`, `Settled`, `AccountClosed`, `AccountReclaimed`, `InsuranceWithdrawn`)

### Keeper bot
- `percli keeper` command with `--json-logs` for structured logging
- Auto-crank and auto-liquidate loop

### Tests
- 20 integration tests covering all on-chain instructions

## 0.7.0

### On-chain program
- Initial Anchor 1.0 program with 8 instructions: `initialize_market`, `deposit`, `withdraw`, `trade`, `crank`, `liquidate`, `settle`, `close_account`
- Pyth oracle integration with staleness and validity checks
- PDA-controlled vault for SPL token custody

### Chain CLI
- `percli chain` subcommand with deploy, deposit, withdraw, trade, crank, liquidate, settle, close, query

### Agent mode
- External process agent protocol (NDJSON stdin/stdout)
- Pyth live feed support (`--features pyth`)

## 0.6.0

### CLI
- `percli step` for incremental state building
- `percli query` for read-only state inspection
- `percli agent` for external process trading agents
- JSON output format (`--format json`)

## 0.5.0

### Core
- Scenario runner with TOML-based scenario files
- Conservation checking and assertion steps
- Bundled scenarios (basic trade, liquidation cascade, haircut stress, insurance depletion, funding drift)

## 0.4.0

### Engine
- Initial release of `percolator-engine` on crates.io
- Formally verified risk engine with Kani proofs
- no_std compatible, zero dependencies
