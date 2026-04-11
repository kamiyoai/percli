# Deploying percli to Solana

This is the operator handbook for deploying the percli on-chain program and a
live market on devnet or mainnet. It walks you from a clean machine to a
running keeper bot, covers the v0.9 → v1.0 migration path, and documents the
production checklists you should run through before sending real value.

> **Audit status:** percli has **not** been audited by a third-party security
> firm. Treat all instructions in this document as the recipe for a sandboxed
> deployment until that changes. Mainnet operators are responsible for their
> own threat modelling.

---

## Table of contents

1. [Prerequisites](#1-prerequisites)
2. [Build the on-chain program](#2-build-the-on-chain-program)
3. [Deploy to devnet](#3-deploy-to-devnet)
4. [Initialize your first market](#4-initialize-your-first-market)
5. [Verify the deployment](#5-verify-the-deployment)
6. [Upgrade an existing program](#6-upgrade-an-existing-program)
7. [Migrating v0.9.x markets to v1.0](#7-migrating-v09x-markets-to-v10)
8. [Authority transfer (two-step)](#8-authority-transfer-two-step)
9. [Run a keeper](#9-run-a-keeper)
10. [Mainnet checklist](#10-mainnet-checklist)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. Prerequisites

| Tool | Version | Why |
|---|---|---|
| Rust | 1.79+ stable | Workspace MSRV |
| Solana CLI | 2.0.x or newer | Wallet, RPC, program deploy |
| `cargo-build-sbf` | bundled with Solana CLI | Compiles the program for the SBF target |
| `solana-keygen` | bundled with Solana CLI | Generates the program keypair |
| (optional) Anchor CLI 1.0 | only if you want to regenerate IDLs |
| (optional) `jq` | for parsing JSON CLI output |

Install the Solana toolchain:

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/v2.2.0/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
```

Install percli:

```bash
# from crates.io (with chain + pyth features)
cargo install percli --features "chain pyth"

# or from source
git clone https://github.com/kamiyoai/percli
cd percli
cargo install --path crates/percli --features "chain pyth"
```

Sanity-check your environment:

```bash
solana --version           # solana-cli 2.x
cargo build-sbf --version  # solana-cargo-build-sbf 2.x
percli --version           # percli 1.0.0
```

---

## 2. Build the on-chain program

The program lives in `crates/percli-program` and declares its program ID in
`src/lib.rs`. The default ID is the public devnet ID
`PercQhVBxXnVCaAhfrPZFc2dVZcQANnwEYroogLJFwm`. **You should not deploy under
this ID unless you also control the upgrade authority for it.** For your own
deployments, generate a fresh keypair (see step 3) and update `declare_id!`
in `crates/percli-program/src/lib.rs` *and* the `[programs.localnet]` /
`[programs.devnet]` section of `Anchor.toml` before building.

Build the SBF artifact:

```bash
cargo build-sbf --skip-tools-install --manifest-path crates/percli-program/Cargo.toml
```

This produces `target/deploy/percli_program.so`. The binary is reproducible
under the same toolchain version — if you need bit-for-bit reproducibility for
audit purposes, pin the Solana CLI and Rust toolchain in CI (see
`.github/workflows/sbf.yml`).

---

## 3. Deploy to devnet

### 3.1. Generate the program keypair

```bash
solana-keygen new --outfile target/deploy/percli_program-keypair.json
solana address -k target/deploy/percli_program-keypair.json
# → prints the new program ID
```

Update `declare_id!` in `crates/percli-program/src/lib.rs` and the program ID
in `Anchor.toml` to match this address, then **rebuild** (step 2). The on-chain
discriminator and PDA derivation depend on the program ID, so any mismatch
will silently break instruction dispatch.

### 3.2. Fund your deploy wallet

```bash
solana config set --url devnet
solana-keygen new --outfile ~/.config/solana/id.json   # if you don't have one
solana airdrop 5                                        # 5 SOL on devnet
```

Program deploys cost ~5 SOL on devnet (rent for ~1 MB of program data).

### 3.3. Deploy the program

```bash
solana program deploy \
    --program-id target/deploy/percli_program-keypair.json \
    target/deploy/percli_program.so
```

You should see `Program Id: <YOUR_PROGRAM_ID>`. The upgrade authority is
your default `solana config get` keypair unless you pass `--upgrade-authority`.

---

## 4. Initialize your first market

A market is a single perp pair with one collateral token, one Pyth oracle,
and one matcher signing key. The on-chain account is a PDA derived from
`["market", authority]` and a token vault PDA derived from
`["vault", market]`.

### 4.1. Configure the chain client

`percli chain` reads connection settings from environment variables and the
default Solana CLI config. The minimum set:

```bash
export PERCLI_RPC_URL=https://api.devnet.solana.com
export PERCLI_PROGRAM_ID=<YOUR_PROGRAM_ID>
export PERCLI_KEYPAIR=$HOME/.config/solana/id.json
```

The signer of `percli chain deploy` becomes the market `authority`. Keep this
key safe — it controls all parameter updates, oracle/matcher rotation, and
authority transfers. You can rotate it later using
[the two-step authority transfer](#8-authority-transfer-two-step).

### 4.2. Pick the market parameters

You need three pubkeys before deploying:

| Field | Devnet example | Notes |
|---|---|---|
| `--mint` | `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU` (devnet USDC) | Any SPL token mint |
| `--oracle` | `J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix` (Pyth devnet SOL/USD) | Must be a Pyth `Price` account |
| `--matcher` | `<YOUR_MATCHER_KEY>` | The only signer allowed to call `trade`. Often a multisig or a dedicated keypair held by the matching engine. |

`--init-price` is the bootstrap mark price (in token base units). It's used
to seed the engine's mark estimate before the first `crank` arrives.

### 4.3. Deploy the market

```bash
percli chain deploy \
    --mint   4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU \
    --oracle J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix \
    --matcher $(solana address) \
    --init-price 100000000
```

You should see:

```
Deploying market...
  Authority:  <YOUR_PUBKEY>
  Market PDA: <MARKET_PDA>
  Vault PDA:  <VAULT_PDA>
  Mint:       4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU
  Oracle:     J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix
  Matcher:    <YOUR_PUBKEY>
  RPC: https://api.devnet.solana.com
  Tx: <TX_SIGNATURE>
Market deployed.
```

Under the hood this submits a single transaction with two instructions:

1. `system_program::create_account` allocating
   `8 + 168 + size_of::<RiskEngine>()` bytes for the market PDA (the
   discriminator, the v1 header, and the engine state).
2. `percli_program::initialize_market` which writes the v1 discriminator
   `b"percmrk\x01"`, the header, and runs the engine init.

> **Account size note.** The host-side `size_of::<RiskEngine>()` is ~536 bytes
> larger than the SBF (on-chain) value due to platform alignment differences
> in the engine's `[Account; MAX_ACCOUNTS]` array. The chain client always
> allocates the host-side (larger) value, which the program's
> `data_len() >= MARKET_ACCOUNT_SIZE` constraint accepts. **Don't try to
> hand-roll the create_account size from the SBF constant** — you'll get a
> `ConstraintRaw` failure.

---

## 5. Verify the deployment

```bash
percli chain query market --address <MARKET_PDA>
```

You should see the freshly initialized header (authority, mint, oracle,
matcher, `pending_authority = 11111111111111111111111111111111`) and zeroed
engine counters.

To inspect the raw account on a block explorer:

```bash
solana account <MARKET_PDA> --output json | jq '.account.data[0]' | head -c 32
# → "cGVyY21yawE="  (base64 of `percmrk\x01`, the v1 discriminator)
```

The 8th byte (`0x01`) is the layout version. Pre-v1 markets show `0x74`
(`'t'`) and must be migrated — see [section 7](#7-migrating-v09x-markets-to-v10).

---

## 6. Upgrade an existing program

### 6.1. Build the new version

```bash
cargo build-sbf --skip-tools-install --manifest-path crates/percli-program/Cargo.toml
```

### 6.2. Upgrade

```bash
solana program deploy \
    --program-id <YOUR_PROGRAM_ID> \
    --upgrade-authority ~/.config/solana/id.json \
    target/deploy/percli_program.so
```

The upgrade authority defaults to whoever first deployed the program. You can
inspect it via `solana program show <PROGRAM_ID>`.

> **Don't forget the data migration.** A program upgrade only swaps out the
> bytecode — it does **not** rewrite existing market accounts. If your upgrade
> changes the layout (as v0.9 → v1.0 does), you must run `migrate_header_v1`
> on every market account before the new bytecode will accept other
> instructions for that market. See section 7.

---

## 7. Migrating v0.9.x markets to v1.0

v1.0 introduces a `pending_authority` field in `MarketHeader`, expanding it
from 136 bytes to 168 bytes. Migration is a separate, idempotent
authority-only instruction. It does **not** realloc the account (no rent top-up
required), it shifts the engine bytes 32 bytes forward inside the existing
buffer and stamps a new layout-version byte.

### 7.1. Pre-flight check

Before upgrading the program bytecode, snapshot every v0.9 market on your
deployment so you can prove the migration was lossless:

```bash
for market in $(percli chain query markets); do
    solana account "$market" --output json > "snapshots/$market.before.json"
done
```

### 7.2. Run the upgrade

Deploy the v1.0 program bytecode (section 6) **but do not yet trade against
any v0.9 market** — every v0.9 instruction handler now requires the v1
discriminator and will reject the legacy `b"percmrkt"` byte pattern with
`AccountNotFound`.

### 7.3. Migrate each market

```bash
percli chain migrate-header-v1
```

Output:

```
Migrating market header from v0 (136 bytes) to v1 (168 bytes)...
  Tx: <TX_SIGNATURE>
  Engine bytes shifted forward by 32; pending_authority slot added.
  Discriminator version byte stamped to 0x01 (v1).
```

Behind the scenes the handler:

1. Verifies the account is owned by the program.
2. Verifies the discriminator at `[0..7]` is `b"percmrk"` and the version
   byte at `[7]` is `0x74` (v0). Already-v1 accounts fail with
   `AlreadyMigrated`.
3. Re-derives the Market PDA from the v0-encoded authority and verifies the
   stored bump matches the canonical PDA bump (rejects tampered headers
   with `CorruptState`).
4. Verifies the signer matches the v0 header's authority field.
5. Shifts the engine bytes from `[144..)` to `[176..)` via `copy_within`
   (in-place, no realloc).
6. Writes a fresh v1 header with `pending_authority = Pubkey::default()`,
   preserving authority/mint/oracle/matcher/bump/vault_bump.
7. Stamps `data[7] = 0x01`.
8. Emits `HeaderMigrated { authority, market, mint, oracle, matcher, account_size }`.

`migrate_header_v1` is idempotent-by-rejection: a second call returns
`AlreadyMigrated` rather than corrupting state.

### 7.4. Post-flight verification

```bash
for market in $(percli chain query markets); do
    solana account "$market" --output json > "snapshots/$market.after.json"

    # confirm the engine bytes are unchanged after the 32-byte shift
    diff <(jq -r '.account.data[0]' snapshots/$market.before.json | base64 -d | tail -c +145 | xxd) \
         <(jq -r '.account.data[0]' snapshots/$market.after.json  | base64 -d | tail -c +177 | xxd)
done
```

(Adjust offsets if your engine size differs.) `percli chain query market` is
also a quick sanity check.

---

## 8. Authority transfer (two-step)

`transfer_authority` and `accept_authority` implement the standard
propose-then-accept rotation pattern. This is critical: a single-step transfer
can permanently brick a market by handing the authority to a typo'd or
unreachable pubkey.

### 8.1. Initiate

The current authority sets `header.pending_authority` to the new pubkey.
`header.authority` is **not** changed.

```bash
percli chain transfer-authority --new-authority <NEW_PUBKEY>
```

The program emits `AuthorityTransferInitiated { market, old_authority, pending_authority }`.

Self-transfer (`new_authority == header.authority`) is rejected with
`Unauthorized` to keep events clean. If a transfer is already in flight to
some other key, this overwrites the previous pending key — which is the
intended way to change your mind before the new key has accepted.

### 8.2. Cancel (optional)

```bash
percli chain transfer-authority --new-authority 11111111111111111111111111111111
```

Passing the default pubkey (all zeros) clears `pending_authority` and emits
`AuthorityTransferCancelled { market, authority, previous_pending }`. This
works because `accept_authority` rejects the default pubkey as a signer.

### 8.3. Accept

The **new** authority signs the accept call. The CLI uses whatever keypair is
in `PERCLI_KEYPAIR` (or `~/.config/solana/id.json`), so make sure you've
switched contexts to the new key first.

```bash
PERCLI_KEYPAIR=/path/to/new/authority.json percli chain accept-authority
```

The program verifies:

- The discriminator is v1 (`is_v1_market`).
- The signer is not `Pubkey::default()` (defense in depth — the runtime
  already rejects this).
- `header.pending_authority != Pubkey::default()` (else `NoPendingAuthority`).
- `header.pending_authority == signer` (else `Unauthorized`).

On success it rotates `header.authority`, clears `header.pending_authority`,
and emits `AuthorityAccepted { market, old_authority, new_authority }`.

### 8.4. Verification

```bash
percli chain query market --address <MARKET_PDA>
# authority should now be NEW_PUBKEY
# pending_authority should be 11111111111111111111111111111111
```

---

## 9. Run a keeper

The keeper bot polls Pyth for the latest oracle price and submits `crank`
calls at a configurable interval. It also auto-liquidates undercollateralized
accounts and emits structured JSON logs suitable for shipping into a log
aggregator.

```bash
percli keeper \
    --rpc https://api.devnet.solana.com \
    --pyth-feed J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix \
    --interval 10 \
    --json-logs
```

Recommended deployment shapes:

- **Devnet**: a single keeper on a small VM, intervals of 10–30 seconds.
- **Mainnet**: redundant keepers in different regions with intervals of
  1–5 seconds, fronted by a leader-election layer (e.g. systemd timer with
  jitter, or a small consensus loop). The on-chain program is idempotent
  against duplicate cranks, so the failure mode of two keepers landing in
  the same slot is just wasted compute.

Run the keeper as the **matcher** keypair, not the authority — this keeps
your authority key offline.

---

## 10. Mainnet checklist

Treat this as the gate between "it ran on devnet" and "it touches user funds".

- [ ] **Audit.** Don't ship without one. Until then, deployments are
      sandboxes.
- [ ] **Fresh program ID.** Generated specifically for the deployment, not
      reused from devnet.
- [ ] **Upgrade authority on a hardware wallet or multisig.** Squads, Realms,
      or a Ledger-backed key. Never the same key as the deploy fee payer.
- [ ] **Authority key offline.** The market authority should live on a
      hardware wallet or multisig and only sign parameter updates and
      key rotations. Day-to-day operations run as the matcher.
- [ ] **Matcher key on the matching engine host.** Rotated quarterly via
      `update_matcher`.
- [ ] **Oracle feed validated.** Confirm you're using a Pyth `Price` account
      (not a `PriceFeed` reference), the publisher set is the production set,
      and `MAX_PRICE_AGE_SECS` (60s in code) matches your Pyth SLA.
- [ ] **Insurance fund seeded.** The first deposit into the insurance vault
      should be made via `top_up_insurance` from a treasury account. The
      `insurance_floor` parameter prevents the fund from being drained below
      this level by `withdraw_insurance`.
- [ ] **Risk parameters reviewed.** `maintenance_margin_bps`,
      `initial_margin_bps`, `liquidation_fee_bps`, `liquidation_fee_cap`,
      `max_crank_staleness_slots` — every value should have an owner who can
      explain why that number.
- [ ] **Keeper redundancy in place.** At minimum two keepers in different
      regions, both monitored.
- [ ] **Crank cadence verified.** A long crank gap can wedge mark prices.
      `accrue_market` is permissionless — ensure your monitoring will trip
      an alarm before `max_crank_staleness_slots` lapses.
- [ ] **Authority transfer drill.** Run a full
      `transfer-authority → accept-authority` cycle on devnet **before**
      mainnet, with the same keys you'll use in production.
- [ ] **Backup snapshots.** Daily `solana account <MARKET_PDA>` snapshots
      and engine event tail to S3 / object store.
- [ ] **Incident runbook.** Documented procedures for: keeper down, oracle
      down, mass-liquidation event, authority key compromise.

---

## 11. Troubleshooting

### `Error: Account not found` from `percli chain deploy`

The program ID in `Anchor.toml` / `lib.rs` doesn't match the deployed program.
Re-check `solana program show <PROGRAM_ID>` and the `declare_id!` macro,
rebuild, and redeploy.

### `ConstraintRaw` failure on `initialize_market`

The chain client allocated an account smaller than the program expects. You're
probably running an out-of-date `percli chain` against a v1.0 program. Upgrade
to `percli >= 1.0.0`.

### `AlreadyMigrated` on `migrate_header_v1`

The market is already at the v1 layout. Nothing to do.

### `NotLegacyLayout` on `migrate_header_v1`

The market discriminator is `b"percmrk"` but the version byte at offset `[7]`
is neither `0x74` (v0) nor `0x01` (v1) — the account data is corrupt or
forged. Investigate before attempting any further mutation.

### `CorruptState` on `migrate_header_v1`

The v0 header's `bump` byte doesn't match the canonical PDA bump for the
encoded authority. The account is either corrupted on disk or was created
under a different program ID. Do **not** force-migrate; investigate.

### `NoPendingAuthority` on `accept_authority`

There's no in-flight transfer. Make sure the current authority ran
`transfer-authority --new-authority <YOUR_KEY>` first.

### `Unauthorized` on `accept_authority`

The signer doesn't match `header.pending_authority`. Check the keypair you're
signing with (`PERCLI_KEYPAIR` env var) and confirm the pending key with
`percli chain query market`.

### `StaleOracle` on `crank` / `accrue_market`

The Pyth price account hasn't published a new value within
`max_crank_staleness_slots`. Either the oracle is down or you're cranking
against the wrong feed. Check `solana account <ORACLE_PUBKEY>` and confirm
the timestamp.

### Keeper logs full of `Insufficient compute budget`

The transaction simulation is hitting the default 200k CU limit. Add a
`ComputeBudgetInstruction::set_compute_unit_limit(400_000)` prefix in
`crates/percli-chain/src/rpc.rs` or pass `--compute-unit-limit` to
`solana program deploy` for the deploy itself.

---

For the full instruction-by-instruction ABI, including discriminators,
account orders, and emitted events, see [`ABI.md`](./ABI.md).
