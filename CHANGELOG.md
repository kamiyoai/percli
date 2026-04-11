# Changelog

## 1.0.0

First production-ready release. Adds operational lifecycle management
(authority transfer, header migration), comprehensive operator and integrator
documentation, and a hardened CI pipeline that builds and tests the on-chain
program end-to-end.

### New on-chain instructions
- `migrate_header_v1` — authority-only; in-place upgrades a v0.9.x market header to v1.0 layout (mint/oracle/matcher promoted into the header, version byte stamped). No realloc; PDA bump is re-validated against `find_program_address` to refuse corrupted accounts.
- `transfer_authority` — authority-only; proposes a new authority via two-step handoff. Self-transfer is rejected; passing `Pubkey::default()` cancels any pending handoff and emits a distinct `AuthorityTransferCancelled` event.
- `accept_authority` — pending authority must sign to take ownership; explicitly rejects `Pubkey::default()` as defense in depth.

### Header v0 → v1 migration
- New version-byte scheme: v0 markets are tagged `b"percmrkt"` (legacy), v1 markets are stamped `b"percmrk\x01"`. All 17 read paths now branch on `is_v1_market()` so v0 and v1 markets coexist on the same program ID during migration windows.
- Migration is in-place: old data is rewritten via `data.copy_within()`. No account realloc, no rent top-up, no downtime.
- Resolves a subtle host/SBF mismatch: `size_of::<RiskEngine>()` differs by ~536 bytes between host and SBF builds due to `i128`/`u128` alignment in the large `[Account; MAX_ACCOUNTS]` array. Allocators on the host side (`percli chain deploy`) now use the v1 constant, which is *strictly larger* than the SBF requirement, so allocations always satisfy the on-chain size check.

### Events
- New events: `HeaderMigrated`, `AuthorityTransferInitiated`, `AuthorityTransferCancelled`, `AuthorityAccepted`. All four carry the market pubkey for indexer-friendliness; `HeaderMigrated` additionally reports the post-migration mint/oracle/matcher and the actual account size.

### Chain CLI
- 3 new `percli chain` subcommands: `migrate-header-v1`, `transfer-authority`, `accept-authority`.
- `percli chain deploy` now allocates with the v1 size constant. **Operators upgrading from 0.9.x must redeploy the program binary; no client-side workaround.**

### Production hardening
- Self-transfer rejection in `transfer_authority` (caught by `Unauthorized`).
- Defense-in-depth `Pubkey::default()` rejection in `accept_authority` even though `transfer_authority` already routes default → cancel.
- `migrate_header_v1` re-derives the market PDA via `find_program_address` and refuses execution if the on-disk bump byte does not match (`CorruptState`). Detects manipulated or corrupted v0 accounts before any in-place rewrite.
- Removed unused `system_program` account from `migrate_header_v1` (no realloc means no allocator needed). Chain client and tests updated to match.

### Tests
- 13 new integration tests (42 total): full v0→v1 migration round-trip, repeated/idempotent migration handling, authority transfer happy path, cancel via default pubkey, overwrite of pending handoff, self-transfer rejection, corrupted-bump rejection, unauthorized callers, double-accept, accept-after-cancel.

### Documentation
- **`DEPLOYMENT.md`** (new) — comprehensive operator handbook covering build, devnet deploy, market initialization, verification, program upgrades, v0.9 → v1.0 migration, two-step authority transfer, keeper operation, mainnet checklist, and a troubleshooting section keyed on every v1.0 error code.
- **`ABI.md`** (new) — canonical wire-format reference for all 19 instructions: account lists, Borsh argument layouts, account-data layout, MarketHeader fields, PDA derivation, error codes (6000–6019), discriminator quick-reference table, and Python verification snippet.

### CI
- New `sbf` GitHub Actions job builds the on-chain program with `cargo build-sbf` and runs the on-chain integration test suite (`cargo test -p percli-program --test integration`). Solana CLI install is cached across runs to keep the job fast.

### Breaking changes
- Markets created on v0.9.x must call `migrate_header_v1` before any other v1.0 instruction will accept them. The on-chain program will continue to refuse v0 markets for write paths until they have been migrated.
- `migrate_header_v1` accounts struct no longer takes `system_program`. Clients calling the instruction directly (not via `percli chain`) must drop that account from their instruction-account list.

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
