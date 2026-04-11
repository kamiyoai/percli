# percli Program ABI (v1.0.0)

This is the canonical wire-format reference for the percli Solana program. It
documents every instruction's discriminator, account list, argument layout,
emitted events, error codes, and the engine call it dispatches to. Use this
document when:

- Building an off-chain client in a language other than Rust.
- Auditing a third-party integration that talks to percli directly.
- Reproducing transactions for forensic analysis.

For the high-level operator workflow, see [`DEPLOYMENT.md`](./DEPLOYMENT.md).

> **Layout version:** v1 (`MARKET_DISCRIMINATOR_V1 = b"percmrk\x01"`).
> Programs running v0.9.x markets must run [`migrate_header_v1`](#19-migrate_header_v1)
> before any other instruction will succeed.

---

## Conventions

### Account modifiers

| Symbol | Meaning |
|---|---|
| `s` | signer |
| `w` | writable |
| `r` | read-only |

`[ws]` = writable signer, `[r]` = read-only non-signer, etc.

### Argument encoding

All arguments are Borsh-encoded in the order listed and prefixed with the
8-byte Anchor discriminator (`sha256("global:<snake_case_name>")[0..8]`).
The full instruction data layout is:

```
<8-byte discriminator><borsh args>
```

### Account-data layout

Every market PDA has the same on-disk layout:

```
[0..8)              discriminator (b"percmrk" + version byte)
[8..176)            MarketHeader (168 bytes, v1 layout)
[176..176 + E)      RiskEngine state (E = SBF size_of::<RiskEngine>())
```

### `MarketHeader` fields (168 bytes, Borsh-encoded)

| Offset (within header) | Size | Field | Type |
|---|---|---|---|
| 0 | 32 | `authority` | `Pubkey` |
| 32 | 32 | `mint` | `Pubkey` |
| 64 | 32 | `oracle` | `Pubkey` |
| 96 | 32 | `matcher` | `Pubkey` |
| 128 | 32 | `pending_authority` | `Pubkey` |
| 160 | 1 | `bump` | `u8` |
| 161 | 1 | `vault_bump` | `u8` |
| 162 | 6 | `_padding` | `[u8; 6]` |

Add 8 to each offset for the absolute position inside the account data buffer.

### PDA derivation

| PDA | Seeds | Description |
|---|---|---|
| Market | `[b"market", authority]` | Stores header + engine; `bump` is recorded in the header. |
| Vault | `[b"vault", market]` | SPL token account holding all collateral; `vault_bump` is recorded in the header. |

### Error codes

| Code | Variant | Meaning |
|---:|---|---|
| 6000 | `InsufficientBalance` | Not enough free collateral / vault funds |
| 6001 | `Undercollateralized` | Operation would leave the account below maintenance margin |
| 6002 | `Unauthorized` | Signer doesn't match the required authority/owner/matcher |
| 6003 | `InvalidMatchingEngine` | Trade legs are invalid (size, account kind, etc.) |
| 6004 | `PnlNotWarmedUp` | PnL conversion attempted before the warmup period elapsed |
| 6005 | `Overflow` | Engine arithmetic overflowed |
| 6006 | `AccountNotFound` | Discriminator/owner/size mismatch, or engine slot empty |
| 6007 | `NotAnLPAccount` | Operation requires an LP account but the slot is a trader |
| 6008 | `PositionSizeMismatch` | Trade leg sizes don't sum to zero |
| 6009 | `AccountKindMismatch` | Wrong AccountKind for this operation |
| 6010 | `SideBlocked` | Side is in drain-only or reset-pending mode |
| 6011 | `CorruptState` | Header deserialization or PDA bump check failed |
| 6012 | `InvalidOraclePrice` | Pyth account is malformed or status is not Trading |
| 6013 | `AccountIndexOutOfRange` | `account_idx >= max_accounts` |
| 6014 | `StaleOracle` | Pyth timestamp is too old |
| 6015 | `InvalidOraclePriceValue` | Oracle price is zero or negative |
| 6016 | `AmountOverflow` | Token amount exceeds `u64::MAX` |
| 6017 | `AlreadyMigrated` | `migrate_header_v1` called on a v1 account |
| 6018 | `NotLegacyLayout` | `migrate_header_v1` called on an account whose discriminator isn't v0 |
| 6019 | `NoPendingAuthority` | `accept_authority` called when `pending_authority == default` |

---

## 1. `initialize_market`

**Discriminator:** `0x2323bdc19b30aacb` (`sha256("global:initialize_market")[0..8]`)
**Auth:** Authority (becomes `header.authority`)
**Engine call:** `RiskEngine::init_in_place(params, init_slot, init_oracle_price)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `authority` | Pays for the vault token account; becomes the market authority |
| 1 | `[w]` | `market` | Market PDA (must be pre-created by the client at `8 + 168 + size_of::<RiskEngine>()` bytes) |
| 2 | `[r]` | `mint` | SPL token mint for collateral |
| 3 | `[w]` | `vault` | Vault token account PDA (initialized by this instruction) |
| 4 | `[r]` | `oracle` | Pyth `Price` account (validated on later cranks) |
| 5 | `[r]` | `matcher` | Matcher signing key (stored in header) |
| 6 | `[r]` | `token_program` | SPL Token program |
| 7 | `[r]` | `system_program` | System program |

### Args

```rust
struct InitializeMarketArgs {
    init_slot: u64,
    init_oracle_price: u64,
    params: RiskParamsInput, // 15 × u64, see below
}

struct RiskParamsInput {
    warmup_period_slots: u64,
    maintenance_margin_bps: u64,
    initial_margin_bps: u64,
    trading_fee_bps: u64,
    max_accounts: u64,
    new_account_fee: u64,
    maintenance_fee_per_slot: u64,
    max_crank_staleness_slots: u64,
    liquidation_fee_bps: u64,
    liquidation_fee_cap: u64,
    min_liquidation_abs: u64,
    min_initial_deposit: u64,
    min_nonzero_mm_req: u64,
    min_nonzero_im_req: u64,
    insurance_floor: u64,
}
```

### Events

```rust
MarketInitialized {
    authority: Pubkey,
    mint: Pubkey,
    oracle: Pubkey,
    matcher: Pubkey,
    init_slot: u64,
    init_oracle_price: u64,
}
```

### Errors

| Code | When |
|---|---|
| `AccountNotFound` (6006) | Market account is too small, has the wrong owner, or has been initialized already |
| `InvalidOraclePriceValue` (6015) | `init_oracle_price == 0` |

---

## 2. `deposit`

**Discriminator:** `0xf223c68952e1f2b6`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::deposit(account_idx, amount, ...)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `user` | Account owner (signer) |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `mint` | Collateral mint |
| 3 | `[w]` | `user_token_account` | Source token account |
| 4 | `[w]` | `vault` | Vault token account PDA |
| 5 | `[r]` | `token_program` | SPL Token program |

### Args

```rust
struct DepositArgs { account_idx: u16, amount: u64 }
```

### Events

```rust
Deposited { user: Pubkey, account_idx: u16, amount: u64 }
```

### Errors

`AccountNotFound`, `Unauthorized`, `AccountIndexOutOfRange`, `AmountOverflow`,
`InsufficientBalance`, plus any underlying SPL token transfer error.

---

## 3. `withdraw`

**Discriminator:** `0xb712469c946da122`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::withdraw(account_idx, amount, funding_rate, ...)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `user` | Account owner |
| 1 | `[w]` | `market` | Market PDA (signs the SPL transfer via vault PDA) |
| 2 | `[r]` | `mint` | Collateral mint |
| 3 | `[w]` | `user_token_account` | Destination token account |
| 4 | `[w]` | `vault` | Vault token account PDA |
| 5 | `[r]` | `token_program` | SPL Token program |

### Args

```rust
struct WithdrawArgs { account_idx: u16, amount: u64, funding_rate: i64 }
```

### Events

```rust
Withdrawn { user: Pubkey, account_idx: u16, amount: u64 }
```

### Errors

`Unauthorized`, `Undercollateralized`, `InsufficientBalance`,
`AccountIndexOutOfRange`, `AmountOverflow`.

---

## 4. `trade`

**Discriminator:** `0xb2901ad8f1bbce82`
**Auth:** Matcher (signer must equal `header.matcher`)
**Engine call:** `RiskEngine::trade(a, b, size_q, exec_price, funding_rate, ...)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `authority` | Matcher signing key |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct TradeArgs {
    account_a: u16,
    account_b: u16,
    size_q: i128,
    exec_price: u64,
    funding_rate: i64,
}
```

### Events

```rust
TradeExecuted { matcher: Pubkey, account_a: u16, account_b: u16, size_q: i128, exec_price: u64 }
```

### Errors

`Unauthorized`, `InvalidMatchingEngine`, `PositionSizeMismatch`,
`AccountKindMismatch`, `SideBlocked`, `Undercollateralized`,
`AccountIndexOutOfRange`.

---

## 5. `crank`

**Discriminator:** `0x00e803c37c756935`
**Auth:** Permissionless
**Engine call:** `RiskEngine::crank(oracle_price, slot, funding_rate)`

Reads the latest Pyth price and applies it as the new mark.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `cranker` | Anyone (signer) |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `oracle` | Pyth `Price` account; must equal `header.oracle` |

### Args

```rust
struct CrankArgs { funding_rate: i64 }
```

### Events

```rust
Cranked { cranker: Pubkey, oracle_price: u64, slot: u64 }
```

### Errors

`InvalidOraclePrice`, `StaleOracle`, `InvalidOraclePriceValue`,
`Unauthorized` (if oracle pubkey doesn't match header).

---

## 6. `liquidate`

**Discriminator:** `0xdfb3e27d302e274a`
**Auth:** Permissionless
**Engine call:** `RiskEngine::liquidate(account_idx, funding_rate)`
**Returns:** `bool` — `true` if the account was actually liquidated.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `liquidator` | Anyone (signer) |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct LiquidateArgs { account_idx: u16, funding_rate: i64 }
```

### Events

```rust
Liquidated { liquidator: Pubkey, account_idx: u16, liquidated: bool }
```

### Errors

`AccountIndexOutOfRange`, `AccountNotFound`.

---

## 7. `settle`

**Discriminator:** `0xaf2ab957908366d4`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::settle(account_idx, funding_rate)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `user` | Account owner |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct SettleArgs { account_idx: u16, funding_rate: i64 }
```

### Events

```rust
Settled { user: Pubkey, account_idx: u16 }
```

### Errors

`Unauthorized`, `AccountIndexOutOfRange`.

---

## 8. `close_account`

**Discriminator:** `0x7dff950e6e224818`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::close_account(account_idx, funding_rate)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `user` | Account owner |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct CloseAccountArgs { account_idx: u16, funding_rate: i64 }
```

### Events

```rust
AccountClosed { user: Pubkey, account_idx: u16 }
```

### Errors

`Unauthorized`, `AccountIndexOutOfRange`, `Undercollateralized`
(if there's residual debt).

---

## 9. `reclaim_account`

**Discriminator:** `0xe6d9263c2b208dd2`
**Auth:** Permissionless
**Engine call:** `RiskEngine::reclaim_account(account_idx)`

Permissionlessly reclaims a slot whose owner has zero collateral, zero
position, and zero fee debt — typically a closed account whose owner has
walked away. Frees the slot for reuse.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `reclaimer` | Anyone (signer) |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct ReclaimAccountArgs { account_idx: u16 }
```

### Events

```rust
AccountReclaimed { reclaimer: Pubkey, account_idx: u16 }
```

### Errors

`AccountIndexOutOfRange`, `AccountNotFound`.

---

## 10. `withdraw_insurance`

**Discriminator:** `0xc9859176eb595abd`
**Auth:** Authority only
**Engine call:** `RiskEngine::withdraw_insurance(amount)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `authority` | Market authority |
| 1 | `[w]` | `market` | Market PDA (signs SPL transfer via vault) |
| 2 | `[r]` | `mint` | Collateral mint |
| 3 | `[w]` | `authority_token_account` | Destination token account |
| 4 | `[w]` | `vault` | Vault token account PDA |
| 5 | `[r]` | `token_program` | SPL Token program |

### Args

```rust
struct WithdrawInsuranceArgs { amount: u64 }
```

### Events

```rust
InsuranceWithdrawn { authority: Pubkey, amount: u64 }
```

### Errors

`Unauthorized`, `InsufficientBalance` (would breach `insurance_floor`).

---

## 11. `top_up_insurance`

**Discriminator:** `0xa5be6dc7a347316e`
**Auth:** Permissionless
**Engine call:** `RiskEngine::top_up_insurance(amount)`

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `depositor` | Anyone with tokens to spare |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `mint` | Collateral mint |
| 3 | `[w]` | `depositor_token_account` | Source token account |
| 4 | `[w]` | `vault` | Vault token account PDA |
| 5 | `[r]` | `token_program` | SPL Token program |

### Args

```rust
struct TopUpInsuranceArgs { amount: u64 }
```

### Events

```rust
InsuranceToppedUp { depositor: Pubkey, amount: u64 }
```

### Errors

`AmountOverflow`, plus SPL transfer errors.

---

## 12. `deposit_fee_credits`

**Discriminator:** `0x7a749118bcace52c`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::deposit_fee_credits(account_idx, amount)`

Lets an account holder pre-pay accumulated fees without touching the trading
collateral.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `user` | Account owner |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `mint` | Collateral mint |
| 3 | `[w]` | `user_token_account` | Source token account |
| 4 | `[w]` | `vault` | Vault token account PDA |
| 5 | `[r]` | `token_program` | SPL Token program |

### Args

```rust
struct DepositFeeCreditsArgs { account_idx: u16, amount: u64 }
```

### Events

```rust
FeeCreditsDeposited { user: Pubkey, account_idx: u16, amount: u64 }
```

### Errors

`Unauthorized`, `AccountIndexOutOfRange`, `AmountOverflow`.

---

## 13. `convert_released_pnl`

**Discriminator:** `0x71b5227b854f5da2`
**Auth:** Account owner (signer)
**Engine call:** `RiskEngine::convert_released_pnl(account_idx, x_req, oracle_price, slot, funding_rate)`

Converts matured (post-warmup) realized PnL into withdrawable collateral.
Reads the current oracle price as part of the conversion math.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `user` | Account owner |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `oracle` | Pyth `Price` account; must equal `header.oracle` |

### Args

```rust
struct ConvertReleasedPnlArgs { account_idx: u16, x_req: u64, funding_rate: i64 }
```

### Events

```rust
PnlConverted { user: Pubkey, account_idx: u16, x_req: u64 }
```

### Errors

`Unauthorized`, `PnlNotWarmedUp`, `AccountIndexOutOfRange`, `StaleOracle`,
`InvalidOraclePrice`, `InvalidOraclePriceValue`.

---

## 14. `accrue_market`

**Discriminator:** `0x67b8692c22f08f59`
**Auth:** Permissionless
**Engine call:** `RiskEngine::accrue_market(oracle_price, slot)`

Mark-to-market + funding accrual without touching individual accounts.
Cheaper than `crank` when you only need to advance the global state.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `signer` | Anyone |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `oracle` | Pyth `Price` account; must equal `header.oracle` |

### Args

None.

### Events

```rust
MarketAccrued { signer: Pubkey, oracle_price: u64, slot: u64 }
```

### Errors

`StaleOracle`, `InvalidOraclePrice`, `InvalidOraclePriceValue`,
`Unauthorized` (oracle pubkey mismatch).

---

## 15. `update_matcher`

**Discriminator:** `0x4d0f04f0c4768b97`
**Auth:** Authority only

Rotates the matcher signing key. The new matcher takes effect immediately.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `authority` | Market authority |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct UpdateMatcherArgs { new_matcher: Pubkey }
```

### Events

```rust
MatcherUpdated { authority: Pubkey, old_matcher: Pubkey, new_matcher: Pubkey }
```

### Errors

`Unauthorized`.

---

## 16. `update_oracle`

**Discriminator:** `0x7029d112f8e2fcbc`
**Auth:** Authority only

Rotates the Pyth oracle account. The new oracle is validated on the next
`crank` / `accrue_market` / `convert_released_pnl` call.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `authority` | Market authority |
| 1 | `[w]` | `market` | Market PDA |
| 2 | `[r]` | `new_oracle` | New Pyth `Price` account (pubkey copied into header) |

### Args

None.

### Events

```rust
OracleUpdated { authority: Pubkey, old_oracle: Pubkey, new_oracle: Pubkey }
```

### Errors

`Unauthorized`.

---

## 17. `transfer_authority`

**Discriminator:** `0x30a94c48e5b437a1`
**Auth:** Authority only

Step 1 of the two-step authority handoff. Writes `new_authority` into
`header.pending_authority` without rotating `header.authority`. Self-transfer
is rejected with `Unauthorized`. Passing `Pubkey::default()` cancels any
in-flight transfer and emits `AuthorityTransferCancelled` instead of
`AuthorityTransferInitiated`.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `authority` | Current market authority |
| 1 | `[w]` | `market` | Market PDA |

### Args

```rust
struct TransferAuthorityArgs { new_authority: Pubkey }
```

### Events

```rust
// Emitted when new_authority != Pubkey::default()
AuthorityTransferInitiated {
    market: Pubkey,
    old_authority: Pubkey,
    pending_authority: Pubkey,
}

// Emitted when new_authority == Pubkey::default()
AuthorityTransferCancelled {
    market: Pubkey,
    authority: Pubkey,
    previous_pending: Pubkey,
}
```

### Errors

`AccountNotFound` (not a v1 market), `Unauthorized` (signer mismatch or
self-transfer), `CorruptState` (header deserialization failed).

---

## 18. `accept_authority`

**Discriminator:** `0x6b56c65b210c6ba0`
**Auth:** Pending authority only

Step 2 of the two-step handoff. Rotates `header.authority` to
`header.pending_authority` and clears `pending_authority`.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[rs]` | `new_authority` | Must equal `header.pending_authority` |
| 1 | `[w]` | `market` | Market PDA |

### Args

None.

### Events

```rust
AuthorityAccepted { market: Pubkey, old_authority: Pubkey, new_authority: Pubkey }
```

### Errors

`AccountNotFound` (not a v1 market), `NoPendingAuthority`
(`pending_authority == default`), `Unauthorized` (signer doesn't match
pending or signer is `Pubkey::default()`).

---

## 19. `migrate_header_v1`

**Discriminator:** `0x4cd8855fd38625cd`
**Auth:** Authority only (must match the v0-encoded authority)

One-time, idempotent-by-rejection migration from the legacy v0.9 layout
(136-byte header) to the v1.0 layout (168-byte header). Performed in-place
via `copy_within` — no realloc and no rent top-up are needed because real
v0.9 mainnet accounts were created with a host-side allocation that's
strictly larger than the SBF v1 size.

### Accounts

| # | Mod | Name | Description |
|---:|---|---|---|
| 0 | `[ws]` | `authority` | Must equal the v0-encoded authority |
| 1 | `[w]` | `market` | v0.9 market PDA (discriminator `b"percmrkt"`) |

### Args

None.

### Algorithm

1. Verify program ownership (Anchor `owner` constraint).
2. Verify discriminator `[0..7] == b"percmrk"` and version byte `[7]`:
   - `0x01` → `AlreadyMigrated`.
   - non-`0x74` → `NotLegacyLayout`.
3. Re-derive the Market PDA from the v0-encoded `authority` and verify the
   stored bump matches the canonical PDA bump (`CorruptState` if not).
4. Verify the signer matches the v0-encoded authority.
5. Shift engine bytes from `[144..)` to `[176..)` via `copy_within`.
6. Write the v1 header at `[8..176)` with `pending_authority = default`,
   preserving authority/mint/oracle/matcher/bump/vault_bump.
7. Stamp `data[7] = 0x01`.

### Events

```rust
HeaderMigrated {
    authority: Pubkey,
    market: Pubkey,
    mint: Pubkey,
    oracle: Pubkey,
    matcher: Pubkey,
    /// Actual on-chain data.len() after migration. Migration is in-place,
    /// so this equals the v0 host-side account size (NOT the SBF v1 size).
    account_size: u64,
}
```

### Errors

`AccountNotFound` (wrong owner, too small, or `[0..7] != b"percmrk"`),
`AlreadyMigrated` (version byte already `0x01`), `NotLegacyLayout`
(version byte is neither `0x01` nor `0x74`), `Unauthorized` (signer mismatch),
`CorruptState` (PDA bump mismatch).

---

## Discriminator quick-reference

| Instruction | Discriminator |
|---|---|
| `initialize_market` | `0x2323bdc19b30aacb` |
| `deposit` | `0xf223c68952e1f2b6` |
| `withdraw` | `0xb712469c946da122` |
| `trade` | `0xb2901ad8f1bbce82` |
| `crank` | `0x00e803c37c756935` |
| `liquidate` | `0xdfb3e27d302e274a` |
| `settle` | `0xaf2ab957908366d4` |
| `close_account` | `0x7dff950e6e224818` |
| `reclaim_account` | `0xe6d9263c2b208dd2` |
| `withdraw_insurance` | `0xc9859176eb595abd` |
| `top_up_insurance` | `0xa5be6dc7a347316e` |
| `deposit_fee_credits` | `0x7a749118bcace52c` |
| `convert_released_pnl` | `0x71b5227b854f5da2` |
| `accrue_market` | `0x67b8692c22f08f59` |
| `update_matcher` | `0x4d0f04f0c4768b97` |
| `update_oracle` | `0x7029d112f8e2fcbc` |
| `migrate_header_v1` | `0x4cd8855fd38625cd` |
| `transfer_authority` | `0x30a94c48e5b437a1` |
| `accept_authority` | `0x6b56c65b210c6ba0` |

These are computed from `sha256("global:<snake_case_name>")[0..8]`. Verify
locally:

```bash
python3 -c '
import hashlib, sys
print(hashlib.sha256(("global:" + sys.argv[1]).encode()).hexdigest()[:16])
' initialize_market
# → 2323bdc19b30aacb
```
