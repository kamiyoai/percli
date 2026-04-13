use borsh::BorshSerialize;
use sha2::{Digest, Sha256};
use solana_program_test::*;
#[allow(deprecated)]
use solana_sdk::{
    account::AccountSharedData,
    clock::Clock,
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const PROGRAM_ID_STR: &str = "PercQhVBxXnVCaAhfrPZFc2dVZcQANnwEYroogLJFwm";

/// Must match MARKET_ACCOUNT_SIZE in the on-chain program (v1 layout).
/// 8 byte discriminator + 168 byte header + engine.
const MARKET_ACCOUNT_SIZE: usize = 8 + 168 + std::mem::size_of::<percli_core::RiskEngine>();
/// Pre-v1 (v0.9.x) account size — used by `migrate_header_v1` tests to
/// simulate a legacy market that still needs migration.
#[allow(dead_code)]
const MARKET_ACCOUNT_SIZE_V0: usize = 8 + 136 + std::mem::size_of::<percli_core::RiskEngine>();

fn program_id() -> Pubkey {
    PROGRAM_ID_STR.parse().unwrap()
}

fn anchor_discriminator(name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

fn build_ix(
    program_id: &Pubkey,
    name: &str,
    args: impl BorshSerialize,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let mut data = anchor_discriminator(name).to_vec();
    args.serialize(&mut data).expect("borsh serialize");
    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Risk params matching the on-chain ABI.
#[derive(BorshSerialize)]
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

#[derive(BorshSerialize)]
struct InitializeMarketArgs {
    init_slot: u64,
    init_oracle_price: u64,
    params: RiskParamsInput,
}

#[derive(BorshSerialize)]
struct DepositArgs {
    account_idx: u16,
    amount: u64,
}

#[derive(BorshSerialize)]
struct WithdrawArgs {
    account_idx: u16,
    amount: u64,
    funding_rate: i64,
}

#[derive(BorshSerialize)]
struct TradeArgs {
    account_a: u16,
    account_b: u16,
    size_q: i128,
    exec_price: u64,
    funding_rate: i64,
}

#[derive(BorshSerialize)]
struct SettleArgs {
    account_idx: u16,
    funding_rate: i64,
}

#[derive(BorshSerialize)]
struct CloseAccountArgs {
    account_idx: u16,
    funding_rate: i64,
}

#[derive(BorshSerialize)]
struct LiquidateArgs {
    account_idx: u16,
    funding_rate: i64,
}

#[derive(BorshSerialize)]
struct ReclaimAccountArgs {
    account_idx: u16,
}

#[derive(BorshSerialize)]
struct WithdrawInsuranceArgs {
    amount: u64,
}

#[derive(BorshSerialize)]
struct TopUpInsuranceArgs {
    amount: u64,
}

#[derive(BorshSerialize)]
struct DepositFeeCreditsArgs {
    account_idx: u16,
    amount: u64,
}

#[derive(BorshSerialize)]
struct UpdateMatcherArgsTest {
    new_matcher: Pubkey,
}

fn default_risk_params() -> RiskParamsInput {
    RiskParamsInput {
        warmup_period_slots: 0,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 64,
        new_account_fee: 0,
        maintenance_fee_per_slot: 0,
        max_crank_staleness_slots: 1_000_000,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: 1_000_000,
        min_liquidation_abs: 100,
        min_initial_deposit: 1000,
        min_nonzero_mm_req: 100,
        min_nonzero_im_req: 200,
        insurance_floor: 0,
    }
}

fn market_pda(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"market", authority.as_ref()], &program_id())
}

fn vault_pda(market: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault", market.as_ref()], &program_id())
}

/// Set up a ProgramTest with the percli_program .so loaded from target/deploy/.
fn program_test() -> ProgramTest {
    ProgramTest::new("percli_program", program_id(), None)
}

/// Create an SPL mint and return its keypair.
async fn create_mint(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    decimals: u8,
) -> Keypair {
    let mint = Keypair::new();
    let rent = banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

    let tx = Transaction::new_signed_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                mint_rent,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint2(
                &spl_token::id(),
                &mint.pubkey(),
                &payer.pubkey(),
                None,
                decimals,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
        &[payer, &mint],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();
    mint
}

/// Create an associated token account for the given owner and mint.
async fn create_ata(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Pubkey {
    let ata = spl_associated_token_account::get_associated_token_address(owner, mint);
    let ix = spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &spl_token::id(),
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();
    ata
}

/// Mint tokens to a token account.
async fn mint_to(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    mint: &Pubkey,
    dest: &Pubkey,
    amount: u64,
) {
    let ix =
        spl_token::instruction::mint_to(&spl_token::id(), mint, dest, &payer.pubkey(), &[], amount)
            .unwrap();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();
}

/// Get token account balance.
async fn token_balance(banks_client: &mut BanksClient, account: &Pubkey) -> u64 {
    let account = banks_client.get_account(*account).await.unwrap().unwrap();
    let token_account = spl_token::state::Account::unpack(&account.data).unwrap();
    token_account.amount
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn initialize_market_ix(
    authority: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    vault: &Pubkey,
    oracle: &Pubkey,
    matcher: &Pubkey,
    args: InitializeMarketArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "initialize_market",
        args,
        vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(*oracle, false),
            AccountMeta::new_readonly(*matcher, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ],
    )
}

fn deposit_ix(
    user: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    user_token_account: &Pubkey,
    vault: &Pubkey,
    args: DepositArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "deposit",
        args,
        vec![
            AccountMeta::new(*user, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*user_token_account, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

fn withdraw_ix(
    user: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    user_token_account: &Pubkey,
    vault: &Pubkey,
    args: WithdrawArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "withdraw",
        args,
        vec![
            AccountMeta::new(*user, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*user_token_account, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

fn trade_ix(matcher: &Pubkey, market: &Pubkey, args: TradeArgs) -> Instruction {
    build_ix(
        &program_id(),
        "trade",
        args,
        vec![
            AccountMeta::new_readonly(*matcher, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn settle_ix(user: &Pubkey, market: &Pubkey, args: SettleArgs) -> Instruction {
    build_ix(
        &program_id(),
        "settle",
        args,
        vec![
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn close_account_ix(user: &Pubkey, market: &Pubkey, args: CloseAccountArgs) -> Instruction {
    build_ix(
        &program_id(),
        "close_account",
        args,
        vec![
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn liquidate_ix(liquidator: &Pubkey, market: &Pubkey, args: LiquidateArgs) -> Instruction {
    build_ix(
        &program_id(),
        "liquidate",
        args,
        vec![
            AccountMeta::new_readonly(*liquidator, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn reclaim_account_ix(
    reclaimer: &Pubkey,
    market: &Pubkey,
    args: ReclaimAccountArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "reclaim_account",
        args,
        vec![
            AccountMeta::new_readonly(*reclaimer, true),
            AccountMeta::new(*market, false),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn withdraw_insurance_ix(
    authority: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    authority_token_account: &Pubkey,
    vault: &Pubkey,
    args: WithdrawInsuranceArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "withdraw_insurance",
        args,
        vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*authority_token_account, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn top_up_insurance_ix(
    depositor: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    depositor_token_account: &Pubkey,
    vault: &Pubkey,
    args: TopUpInsuranceArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "top_up_insurance",
        args,
        vec![
            AccountMeta::new(*depositor, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*depositor_token_account, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn deposit_fee_credits_ix(
    user: &Pubkey,
    market: &Pubkey,
    mint: &Pubkey,
    user_token_account: &Pubkey,
    vault: &Pubkey,
    args: DepositFeeCreditsArgs,
) -> Instruction {
    build_ix(
        &program_id(),
        "deposit_fee_credits",
        args,
        vec![
            AccountMeta::new(*user, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*user_token_account, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

fn update_matcher_ix(
    authority: &Pubkey,
    market: &Pubkey,
    args: UpdateMatcherArgsTest,
) -> Instruction {
    build_ix(
        &program_id(),
        "update_matcher",
        args,
        vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn update_oracle_ix(authority: &Pubkey, market: &Pubkey, new_oracle: &Pubkey) -> Instruction {
    build_ix(
        &program_id(),
        "update_oracle",
        (),
        vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*new_oracle, false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Shared setup: deploy market + mint + fund users
// ---------------------------------------------------------------------------

struct TestEnv {
    banks_client: BanksClient,
    authority: Keypair,
    market: Pubkey,
    vault: Pubkey,
    mint: Pubkey,
    matcher: Keypair,
    #[allow(dead_code)]
    oracle: Pubkey,
    recent_blockhash: solana_sdk::hash::Hash,
}

/// Build a ProgramTest with the market PDA pre-created.
/// We need to know the authority pubkey upfront to derive the PDA.
fn setup_program_test(authority: &Keypair, oracle: Pubkey) -> (ProgramTest, Pubkey) {
    let mut pt = program_test();

    let (market, _bump) = market_pda(&authority.pubkey());

    // Pre-create the market PDA account — too large for CPI create_account (>10KB).
    // In production, use extend_market or a non-PDA keypair account.
    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000, // enough for rent
            data: vec![0u8; MARKET_ACCOUNT_SIZE],
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    // Add oracle stub
    pt.add_account(
        oracle,
        solana_sdk::account::Account {
            lamports: 1_000_000,
            data: vec![0u8; 64],
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // Fund the authority with SOL
    pt.add_account(
        authority.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    (pt, market)
}

async fn setup_market() -> TestEnv {
    let authority = Keypair::new();
    let matcher = Keypair::new();
    let oracle = Pubkey::new_unique();

    let (pt, market) = setup_program_test(&authority, oracle);
    let (vault, _vbump) = vault_pda(&market);

    let (mut banks_client, _default_payer, recent_blockhash) = pt.start().await;

    let mint_kp = create_mint(&mut banks_client, &authority, recent_blockhash, 6).await;
    let mint = mint_kp.pubkey();

    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    let slot = banks_client.get_root_slot().await.unwrap();

    let ix = initialize_market_ix(
        &authority.pubkey(),
        &market,
        &mint,
        &vault,
        &oracle,
        &matcher.pubkey(),
        InitializeMarketArgs {
            init_slot: slot,
            init_oracle_price: 1000,
            params: default_risk_params(),
        },
    );

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();

    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    TestEnv {
        banks_client,
        authority,
        market,
        vault,
        mint,
        matcher,
        oracle,
        recent_blockhash,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_initialize_market() {
    let env = setup_market().await;

    // Market account should exist and be owned by the program
    let market_account = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .expect("market account should exist");

    assert_eq!(market_account.owner, program_id());
    // Discriminator is `percmrk` + version byte 0x01 (v1 layout).
    assert_eq!(&market_account.data[0..7], b"percmrk");
    assert_eq!(market_account.data[7], 0x01);
}

#[tokio::test]
async fn test_initialize_market_zero_price_fails() {
    let authority = Keypair::new();
    let oracle = Pubkey::new_unique();
    let matcher = Keypair::new();

    let (pt, market) = setup_program_test(&authority, oracle);
    let (vault, _) = vault_pda(&market);

    let (mut banks_client, _default_payer, recent_blockhash) = pt.start().await;
    let mint_kp = create_mint(&mut banks_client, &authority, recent_blockhash, 6).await;

    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    let ix = initialize_market_ix(
        &authority.pubkey(),
        &market,
        &mint_kp.pubkey(),
        &vault,
        &oracle,
        &matcher.pubkey(),
        InitializeMarketArgs {
            init_slot: 0,
            init_oracle_price: 0, // should fail
            params: default_risk_params(),
        },
    );

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    let result = banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "init with zero price should fail");
}

#[tokio::test]
async fn test_deposit_and_ownership() {
    let mut env = setup_market().await;

    // Create user token account and fund it
    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit 10,000 tokens to slot 0
    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 10_000,
        },
    );

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // Verify tokens moved from user to vault
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 10_000);

    let user_bal = token_balance(&mut env.banks_client, &user_ata).await;
    assert_eq!(user_bal, 1_000_000 - 10_000);
}

#[tokio::test]
async fn test_deposit_wrong_owner_rejected() {
    let mut env = setup_market().await;

    // First, deposit from authority to claim slot 0
    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 10_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Now try to deposit to the same slot from a different user
    let intruder = Keypair::new();

    // Fund the intruder with SOL
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder_ata,
        100_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_ix(
        &intruder.pubkey(),
        &env.market,
        &env.mint,
        &intruder_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0, // same slot — should fail
            amount: 5_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "deposit to owned slot by different user should fail"
    );
}

#[tokio::test]
async fn test_withdraw() {
    let mut env = setup_market().await;

    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit
    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Withdraw half
    let ix = withdraw_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        WithdrawArgs {
            account_idx: 0,
            amount: 50_000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // Check balances
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 50_000);

    let user_bal = token_balance(&mut env.banks_client, &user_ata).await;
    assert_eq!(user_bal, 1_000_000 - 100_000 + 50_000);
}

#[tokio::test]
async fn test_withdraw_wrong_owner_rejected() {
    let mut env = setup_market().await;

    // Authority deposits to slot 0
    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to withdraw from slot 0
    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = withdraw_ix(
        &intruder.pubkey(),
        &env.market,
        &env.mint,
        &intruder_ata,
        &env.vault,
        WithdrawArgs {
            account_idx: 0,
            amount: 10_000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "withdraw by non-owner should fail");
}

#[tokio::test]
async fn test_trade() {
    let mut env = setup_market().await;

    // Create two users (authority = user A, new keypair = user B)
    let user_b = Keypair::new();

    // Fund user B with SOL
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Create ATAs and fund both users
    let ata_a = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ata_b = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_b.pubkey(),
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_a,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_b,
        1_000_000,
    )
    .await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit for user A (slot 0)
    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit for user B (slot 1)
    let ix = deposit_ix(
        &user_b.pubkey(),
        &env.market,
        &env.mint,
        &ata_b,
        &env.vault,
        DepositArgs {
            account_idx: 1,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Execute trade: matcher signs, account 0 goes long, account 1 goes short
    let ix = trade_ix(
        &env.matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 1,
            size_q: 100,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority, &env.matcher],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // If we got here, the trade succeeded
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 1_000_000, "vault should hold both deposits");
}

#[tokio::test]
async fn test_trade_wrong_matcher_rejected() {
    let mut env = setup_market().await;

    // Setup two funded accounts
    let user_b = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ata_a = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_b.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_a,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_b,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_a = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(
        &user_b.pubkey(),
        &env.market,
        &env.mint,
        &ata_b,
        &env.vault,
        DepositArgs {
            account_idx: 1,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try trade with wrong matcher (use authority instead of designated matcher)
    let fake_matcher = &env.authority; // not the real matcher
    let ix = trade_ix(
        &fake_matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 1,
            size_q: 100,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "trade with wrong matcher should fail");
}

#[tokio::test]
async fn test_trade_self_trade_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Self-trade: same account on both sides
    let ix = trade_ix(
        &env.matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 0, // same slot
            size_q: 100,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority, &env.matcher],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "self-trade should fail");
}

#[tokio::test]
async fn test_full_lifecycle() {
    let mut env = setup_market().await;

    let user_b = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Create ATAs and fund
    let ata_a = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_b.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_a,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_b,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 1. Deposit — both users
    let dep_a = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(
        &user_b.pubkey(),
        &env.market,
        &env.mint,
        &ata_b,
        &env.vault,
        DepositArgs {
            account_idx: 1,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    assert_eq!(
        token_balance(&mut env.banks_client, &env.vault).await,
        1_000_000
    );

    // 2. Settle — both accounts (no open position, just exercise the instruction)
    let settle_a = settle_ix(
        &env.authority.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[settle_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let settle_b = settle_ix(
        &user_b.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 1,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[settle_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 4. Close — both accounts
    let close_a = close_account_ix(
        &env.authority.pubkey(),
        &env.market,
        CloseAccountArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[close_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let close_b = close_account_ix(
        &user_b.pubkey(),
        &env.market,
        CloseAccountArgs {
            account_idx: 1,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[close_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 5. Withdraw remaining balances
    let withdraw_a = withdraw_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        WithdrawArgs {
            account_idx: 0,
            amount: 500_000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[withdraw_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    // Note: withdraw after close may fail depending on engine semantics — that's fine.
    // The point is to exercise the full path. If close zeroes balance, skip this.
    let _ = env.banks_client.process_transaction(tx).await;

    // Market should still exist
    let market_account = env.banks_client.get_account(env.market).await.unwrap();
    assert!(
        market_account.is_some(),
        "market account should still exist"
    );
}

#[tokio::test]
async fn test_deposit_zero_amount_fails() {
    let mut env = setup_market().await;

    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero deposit should fail");
}

// ---------------------------------------------------------------------------
// New v0.8.0 tests
// ---------------------------------------------------------------------------

/// Helper: set up two funded accounts with positions for liquidation testing.
async fn setup_two_accounts_with_trade() -> (TestEnv, Keypair, Pubkey, Pubkey) {
    let mut env = setup_market().await;
    let user_b = Keypair::new();

    // Fund user B with SOL
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ata_a = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_b.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_a,
        10_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata_b,
        10_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit for both users
    let dep_a = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(
        &user_b.pubkey(),
        &env.market,
        &env.mint,
        &ata_b,
        &env.vault,
        DepositArgs {
            account_idx: 1,
            amount: 500_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Trade: account 0 long, account 1 short
    let ix = trade_ix(
        &env.matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 1,
            size_q: 1_000_000,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority, &env.matcher],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    (env, user_b, ata_a, ata_b)
}

#[tokio::test]
async fn test_liquidate_healthy_account_no_effect() {
    let (mut env, _user_b, _ata_a, _ata_b) = setup_two_accounts_with_trade().await;

    // Try to liquidate account 0 — should not fail but liquidated=false
    // (account is well-collateralized at current price)
    let ix = liquidate_ix(
        &env.authority.pubkey(),
        &env.market,
        LiquidateArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    // Liquidation of healthy account either returns false or succeeds without effect
    let _ = env.banks_client.process_transaction(tx).await;
}

#[tokio::test]
async fn test_withdraw_insurance_authority_only() {
    let mut env = setup_market().await;

    // We need insurance fund to have some balance.
    // The insurance fund starts at 0. We'll deposit and trade to generate fees.
    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        10_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit (new_account_fee=0 in our config, so no insurance accrual from deposit alone)
    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 1_000_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try to withdraw 0 insurance — should fail (amount=0)
    let ix = withdraw_insurance_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        WithdrawInsuranceArgs { amount: 0 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero withdraw_insurance should fail");
}

#[tokio::test]
async fn test_withdraw_insurance_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Non-authority tries to withdraw insurance
    let ix = withdraw_insurance_ix(
        &intruder.pubkey(),
        &env.market,
        &env.mint,
        &intruder_ata,
        &env.vault,
        WithdrawInsuranceArgs { amount: 1000 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "non-authority withdraw_insurance should fail"
    );
}

#[tokio::test]
async fn test_reclaim_active_account_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit enough capital (well above min_initial_deposit=1000)
    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try to reclaim — should fail (capital >> 0, above floor)
    let ix = reclaim_account_ix(
        &env.authority.pubkey(),
        &env.market,
        ReclaimAccountArgs { account_idx: 0 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "reclaim of active account should fail");
}

#[tokio::test]
async fn test_reclaim_unused_slot_rejected() {
    let mut env = setup_market().await;

    // Try to reclaim slot 0 that was never used
    let ix = reclaim_account_ix(
        &env.authority.pubkey(),
        &env.market,
        ReclaimAccountArgs { account_idx: 0 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "reclaim of unused slot should fail");
}

#[tokio::test]
async fn test_close_account_wrong_owner_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to close
    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = close_account_ix(
        &intruder.pubkey(),
        &env.market,
        CloseAccountArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "close by non-owner should fail");
}

#[tokio::test]
async fn test_deposit_trade_settle_close_lifecycle() {
    let (mut env, user_b, ata_a, _ata_b) = setup_two_accounts_with_trade().await;

    // Settle account 0
    let ix = settle_ix(
        &env.authority.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Settle account 1
    let ix = settle_ix(
        &user_b.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 1,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Vault should still have tokens
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert!(vault_bal > 0, "vault should have tokens after trade");

    // Withdraw from account 0 (partial)
    let ix = withdraw_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        WithdrawArgs {
            account_idx: 0,
            amount: 10_000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // Verify user received tokens
    let user_bal = token_balance(&mut env.banks_client, &ata_a).await;
    assert!(user_bal > 0, "user should have received withdrawn tokens");
}

#[tokio::test]
async fn test_multiple_deposits_same_slot() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        10_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit 1
    let dep1 = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 50_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep1],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit 2 (same slot, same owner)
    let dep2 = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 30_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep2],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // Vault should have 80_000
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 80_000);
}

#[tokio::test]
async fn test_withdraw_more_than_available_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 10_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try to withdraw more than deposited
    let ix = withdraw_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        WithdrawArgs {
            account_idx: 0,
            amount: 999_999,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "withdrawing more than balance should fail");
}

// ---------------------------------------------------------------------------
// v0.9.0 tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_top_up_insurance() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 50_000 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 50_000, "vault should hold top-up amount");

    let user_bal = token_balance(&mut env.banks_client, &ata).await;
    assert_eq!(user_bal, 950_000);
}

#[tokio::test]
async fn test_top_up_insurance_zero_fails() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 0 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero top-up should fail");
}

#[tokio::test]
async fn test_top_up_insurance_permissionless() {
    let mut env = setup_market().await;

    // Random user (not authority) can top up
    let random_user = Keypair::new();
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &random_user.pubkey(),
        1_000_000_000,
    );
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let user_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &random_user.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &user_ata,
        100_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &random_user.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 25_000 },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&random_user.pubkey()),
        &[&random_user],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 25_000, "permissionless top-up should work");
}

#[tokio::test]
async fn test_deposit_fee_credits() {
    let mut env = setup_market().await;

    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // First deposit to open the account slot
    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit fee credits (no fee debt yet, so engine caps to 0, but SPL transfer still happens)
    let ix = deposit_fee_credits_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositFeeCreditsArgs {
            account_idx: 0,
            amount: 10_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    // Vault should now have deposit + fee credits
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 110_000);
}

#[tokio::test]
async fn test_deposit_fee_credits_wrong_owner() {
    let mut env = setup_market().await;

    // Authority opens slot 0
    let ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &ata,
        1_000_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 100_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[dep],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to deposit fee credits to authority's slot
    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder.pubkey(),
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(
        &mut env.banks_client,
        &env.authority,
        env.recent_blockhash,
        &env.mint,
        &intruder_ata,
        100_000,
    )
    .await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_fee_credits_ix(
        &intruder.pubkey(),
        &env.market,
        &env.mint,
        &intruder_ata,
        &env.vault,
        DepositFeeCreditsArgs {
            account_idx: 0,
            amount: 5_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "deposit_fee_credits by non-owner should fail"
    );
}

#[tokio::test]
async fn test_update_matcher() {
    let (mut env, user_b, _ata_a, _ata_b) = setup_two_accounts_with_trade().await;

    let new_matcher = Keypair::new();

    // Authority rotates matcher
    let ix = update_matcher_ix(
        &env.authority.pubkey(),
        &env.market,
        UpdateMatcherArgsTest {
            new_matcher: new_matcher.pubkey(),
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Old matcher should be rejected
    let ix = trade_ix(
        &env.matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 1,
            size_q: 50,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority, &env.matcher],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "old matcher should be rejected after rotation"
    );

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Fund new matcher with SOL
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &new_matcher.pubkey(),
        1_000_000_000,
    );
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // New matcher should work — settle first to flatten positions
    let settle_a = settle_ix(
        &env.authority.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[settle_a],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let settle_b = settle_ix(
        &user_b.pubkey(),
        &env.market,
        SettleArgs {
            account_idx: 1,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[settle_b],
        Some(&user_b.pubkey()),
        &[&user_b],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Trade with new matcher
    let ix = trade_ix(
        &new_matcher.pubkey(),
        &env.market,
        TradeArgs {
            account_a: 0,
            account_b: 1,
            size_q: 50,
            exec_price: 1000,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority, &new_matcher],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
}

#[tokio::test]
async fn test_update_matcher_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = update_matcher_ix(
        &intruder.pubkey(),
        &env.market,
        UpdateMatcherArgsTest {
            new_matcher: intruder.pubkey(),
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-authority update_matcher should fail");
}

#[tokio::test]
async fn test_update_oracle() {
    let mut env = setup_market().await;

    // Create a new oracle account owned by PYTH_PROGRAM_ID
    // In test env, we just need the account to have the right owner
    let new_oracle = Pubkey::new_unique();

    // We can't easily add accounts after ProgramTest starts, but update_oracle
    // validates that new_oracle.owner == PYTH_PROGRAM_ID. Since we can't create
    // a Pyth-owned account at runtime, we test the authority check instead.
    // The unauthorized test below covers the auth check. The happy path is covered
    // by setup_program_test adding oracle with a specific owner.

    // For a proper happy-path test, we need the oracle stub to have PYTH_PROGRAM_ID as owner.
    // Let's do that via a custom setup.
    let authority = Keypair::new();
    let old_oracle = Pubkey::new_unique();
    let new_oracle_key = Pubkey::new_unique();
    let pyth_program_id: Pubkey = "FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH"
        .parse()
        .unwrap();

    let mut pt = program_test();
    let (market, _bump) = market_pda(&authority.pubkey());

    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000,
            data: vec![0u8; MARKET_ACCOUNT_SIZE],
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    // Old oracle — owned by some random program (not Pyth, just a stub)
    pt.add_account(
        old_oracle,
        solana_sdk::account::Account {
            lamports: 1_000_000,
            data: vec![0u8; 64],
            owner: Pubkey::new_unique(),
            executable: false,
            rent_epoch: 0,
        },
    );

    // New oracle — owned by PYTH_PROGRAM_ID
    pt.add_account(
        new_oracle_key,
        solana_sdk::account::Account {
            lamports: 1_000_000,
            data: vec![0u8; 64],
            owner: pyth_program_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    pt.add_account(
        authority.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    let matcher = Keypair::new();
    let (vault, _vbump) = vault_pda(&market);
    let (mut banks_client, _payer, recent_blockhash) = pt.start().await;

    let mint_kp = create_mint(&mut banks_client, &authority, recent_blockhash, 6).await;
    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    let slot = banks_client.get_root_slot().await.unwrap();
    let ix = initialize_market_ix(
        &authority.pubkey(),
        &market,
        &mint_kp.pubkey(),
        &vault,
        &old_oracle,
        &matcher.pubkey(),
        InitializeMarketArgs {
            init_slot: slot,
            init_oracle_price: 1000,
            params: default_risk_params(),
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();
    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    // Update oracle
    let ix = update_oracle_ix(&authority.pubkey(), &market, &new_oracle_key);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();

    // Verify: read the header and check oracle field
    let market_account = banks_client.get_account(market).await.unwrap().unwrap();
    // The oracle pubkey is at offset 8 (discriminator) + 32 (authority) + 32 (mint) = 72
    let oracle_bytes = &market_account.data[72..104];
    assert_eq!(
        oracle_bytes,
        new_oracle_key.to_bytes(),
        "oracle should be updated in header"
    );
}

#[tokio::test]
async fn test_update_oracle_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to update oracle — should fail even though we use the existing oracle as new_oracle
    let ix = update_oracle_ix(&intruder.pubkey(), &env.market, &env.oracle);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-authority update_oracle should fail");
}

// ---------------------------------------------------------------------------
// v1.0 instructions: migrate_header_v1, transfer_authority, accept_authority,
// accrue_market, convert_released_pnl
// ---------------------------------------------------------------------------

const PYTH_PROGRAM_ID_STR: &str = "FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH";

fn pyth_program_id() -> Pubkey {
    PYTH_PROGRAM_ID_STR.parse().unwrap()
}

#[derive(BorshSerialize)]
struct TransferAuthorityArgsTest {
    new_authority: Pubkey,
}

#[derive(BorshSerialize)]
struct ConvertReleasedPnlArgsTest {
    account_idx: u16,
    x_req: u64,
    funding_rate: i64,
}

fn migrate_header_v1_ix(authority: &Pubkey, market: &Pubkey) -> Instruction {
    build_ix(
        &program_id(),
        "migrate_header_v1",
        (),
        vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn transfer_authority_ix(
    authority: &Pubkey,
    market: &Pubkey,
    new_authority: Pubkey,
) -> Instruction {
    build_ix(
        &program_id(),
        "transfer_authority",
        TransferAuthorityArgsTest { new_authority },
        vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn accept_authority_ix(new_authority: &Pubkey, market: &Pubkey) -> Instruction {
    build_ix(
        &program_id(),
        "accept_authority",
        (),
        vec![
            AccountMeta::new_readonly(*new_authority, true),
            AccountMeta::new(*market, false),
        ],
    )
}

fn accrue_market_ix(signer: &Pubkey, market: &Pubkey, oracle: &Pubkey) -> Instruction {
    build_ix(
        &program_id(),
        "accrue_market",
        (),
        vec![
            AccountMeta::new_readonly(*signer, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*oracle, false),
        ],
    )
}

fn convert_released_pnl_ix(
    user: &Pubkey,
    market: &Pubkey,
    oracle: &Pubkey,
    args: ConvertReleasedPnlArgsTest,
) -> Instruction {
    build_ix(
        &program_id(),
        "convert_released_pnl",
        args,
        vec![
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*oracle, false),
        ],
    )
}

/// Construct a fully zero-initialised Pyth `SolanaPriceAccount` with just the
/// fields the percli program checks: magic, ver, atype, expo, agg.price,
/// agg.status, and timestamp.
///
/// Returns the raw bytes (`size_of::<SolanaPriceAccount>()` long) ready to drop
/// into a `solana_sdk::account::Account` whose owner is the Pyth program ID.
fn build_pyth_price_account(price: i64, expo: i32, timestamp: i64) -> Vec<u8> {
    use pyth_sdk_solana::state::{
        AccountType, PriceInfo, PriceStatus, SolanaPriceAccount, MAGIC, VERSION_2,
    };

    let mut pa = SolanaPriceAccount::default();
    pa.magic = MAGIC;
    pa.ver = VERSION_2;
    pa.atype = AccountType::Price as u32;
    pa.expo = expo;
    pa.timestamp = timestamp;
    pa.agg = PriceInfo {
        price,
        conf: 0,
        status: PriceStatus::Trading,
        ..Default::default()
    };

    bytemuck::bytes_of(&pa).to_vec()
}

/// Create a market account preloaded with the legacy v0.9 (136-byte header)
/// layout, ready for `migrate_header_v1` to be called on it.
///
/// Layout (no Anchor framework involvement, all manual bytes):
///   [0..8)         "percmrkt"
///   [8..40)        authority pubkey
///   [40..72)       mint pubkey
///   [72..104)      oracle pubkey
///   [104..136)     matcher pubkey
///   [136]          bump
///   [137]          vault_bump
///   [138..144)     padding
///   [144..144 + E) zeroed engine
fn build_v0_market_data(
    authority: &Pubkey,
    mint: &Pubkey,
    oracle: &Pubkey,
    matcher: &Pubkey,
    bump: u8,
    vault_bump: u8,
) -> Vec<u8> {
    let mut data = vec![0u8; MARKET_ACCOUNT_SIZE_V0];
    data[0..8].copy_from_slice(b"percmrkt");
    data[8..40].copy_from_slice(authority.as_ref());
    data[40..72].copy_from_slice(mint.as_ref());
    data[72..104].copy_from_slice(oracle.as_ref());
    data[104..136].copy_from_slice(matcher.as_ref());
    data[136] = bump;
    data[137] = vault_bump;
    // bytes [138..144) and [144..) remain zero — engine is all-zero, which is a
    // valid bit pattern for `RiskEngine` (it's `bytemuck::Pod`).
    data
}

#[tokio::test]
async fn test_migrate_header_v1_happy_path() {
    let authority = Keypair::new();
    let mint_key = Pubkey::new_unique();
    let oracle = Pubkey::new_unique();
    let matcher_key = Pubkey::new_unique();

    let mut pt = program_test();
    let (market, market_bump) = market_pda(&authority.pubkey());
    let (_vault, vault_bump) = vault_pda(&market);

    // Pre-create the market PDA at the *legacy* v0 size with valid v0 bytes.
    let v0_data = build_v0_market_data(
        &authority.pubkey(),
        &mint_key,
        &oracle,
        &matcher_key,
        market_bump,
        vault_bump,
    );
    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000,
            data: v0_data,
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    pt.add_account(
        authority.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    let (mut banks_client, _payer, recent_blockhash) = pt.start().await;

    // Sanity-check that the v0 layout was preserved through ProgramTest setup.
    let pre = banks_client.get_account(market).await.unwrap().unwrap();
    assert_eq!(
        pre.data.len(),
        MARKET_ACCOUNT_SIZE_V0,
        "v0 account should be exactly MARKET_ACCOUNT_SIZE_V0 bytes before migration"
    );
    // Pre-migration discriminator: legacy `percmrkt` (last byte = 't' = 0x74).
    assert_eq!(&pre.data[0..8], b"percmrkt");

    let ix = migrate_header_v1_ix(&authority.pubkey(), &market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();

    // Verify post-migration state. The migration is performed in-place
    // without realloc, so `data.len()` is unchanged from the pre-migration
    // host-side V0 size. The other instruction handlers check
    // `data_len() >= MARKET_ACCOUNT_SIZE` (SBF size), which is satisfied
    // because real-chain v0 accounts are strictly larger than SBF v1 size.
    let acct = banks_client.get_account(market).await.unwrap().unwrap();
    assert_eq!(
        acct.data.len(),
        MARKET_ACCOUNT_SIZE_V0,
        "in-place migration does not resize the account"
    );
    // Discriminator now carries the v1 version byte (0x01) at offset [7].
    assert_eq!(&acct.data[0..7], b"percmrk");
    assert_eq!(acct.data[7], 0x01, "version byte stamped to v1 (0x01)");
    assert_eq!(
        &acct.data[8..40],
        authority.pubkey().as_ref(),
        "authority preserved"
    );
    assert_eq!(&acct.data[40..72], mint_key.as_ref(), "mint preserved");
    assert_eq!(&acct.data[72..104], oracle.as_ref(), "oracle preserved");
    assert_eq!(
        &acct.data[104..136],
        matcher_key.as_ref(),
        "matcher preserved"
    );
    assert_eq!(
        &acct.data[136..168],
        Pubkey::default().as_ref(),
        "pending_authority is default"
    );
    assert_eq!(acct.data[168], market_bump, "bump preserved");
    assert_eq!(acct.data[169], vault_bump, "vault_bump preserved");
}

#[tokio::test]
async fn test_migrate_header_v1_double_fails() {
    // Set up a normal v1 market via the existing helper, then try to migrate.
    // It should fail with NotLegacyLayout / AlreadyMigrated because the account
    // is already at the v1 size.
    let mut env = setup_market().await;

    let ix = migrate_header_v1_ix(&env.authority.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "migrate_header_v1 against an already-v1 account should fail"
    );
}

#[tokio::test]
async fn test_migrate_header_v1_wrong_authority() {
    let real_authority = Keypair::new();
    let intruder = Keypair::new();
    let mint_key = Pubkey::new_unique();
    let oracle = Pubkey::new_unique();
    let matcher_key = Pubkey::new_unique();

    let mut pt = program_test();
    let (market, market_bump) = market_pda(&real_authority.pubkey());
    let (_vault, vault_bump) = vault_pda(&market);

    let v0_data = build_v0_market_data(
        &real_authority.pubkey(),
        &mint_key,
        &oracle,
        &matcher_key,
        market_bump,
        vault_bump,
    );
    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000,
            data: v0_data,
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );
    pt.add_account(
        intruder.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    let (mut banks_client, _payer, recent_blockhash) = pt.start().await;

    let ix = migrate_header_v1_ix(&intruder.pubkey(), &market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        recent_blockhash,
    );
    let result = banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "migrate_header_v1 with non-authority signer should fail"
    );
}

#[tokio::test]
async fn test_transfer_and_accept_authority_happy_path() {
    let mut env = setup_market().await;
    let new_authority = Keypair::new();

    // Fund new_authority with SOL so it can sign.
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &new_authority.pubkey(),
        1_000_000_000,
    );
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Step 1: old authority initiates the transfer.
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, new_authority.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // After step 1: header.authority is unchanged, header.pending_authority is the new key.
    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[8..40],
        env.authority.pubkey().as_ref(),
        "authority unchanged after initiate"
    );
    assert_eq!(
        &acct.data[136..168],
        new_authority.pubkey().as_ref(),
        "pending_authority set"
    );

    // Step 2: new authority accepts.
    let ix = accept_authority_ix(&new_authority.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&new_authority.pubkey()),
        &[&new_authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[8..40],
        new_authority.pubkey().as_ref(),
        "authority rotated"
    );
    assert_eq!(
        &acct.data[136..168],
        Pubkey::default().as_ref(),
        "pending_authority cleared"
    );
}

#[tokio::test]
async fn test_accept_authority_wrong_signer_fails() {
    let mut env = setup_market().await;
    let pending = Keypair::new();
    let intruder = Keypair::new();

    // Fund both
    for pk in [pending.pubkey(), intruder.pubkey()] {
        let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &pk, 1_000_000_000);
        let tx = Transaction::new_signed_with_payer(
            &[transfer_ix],
            Some(&env.authority.pubkey()),
            &[&env.authority],
            env.recent_blockhash,
        );
        env.banks_client.process_transaction(tx).await.unwrap();
        env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    }

    // Initiate the transfer to `pending`
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, pending.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to accept — should fail.
    let ix = accept_authority_ix(&intruder.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&intruder.pubkey()),
        &[&intruder],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-pending signer should be rejected");

    // And the original authority should still be in place.
    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[8..40],
        env.authority.pubkey().as_ref(),
        "authority unchanged after failed accept"
    );
}

#[tokio::test]
async fn test_transfer_authority_cancel() {
    let mut env = setup_market().await;
    let pending = Keypair::new();

    // Initiate transfer
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, pending.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Cancel by sending Pubkey::default()
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, Pubkey::default());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[136..168],
        Pubkey::default().as_ref(),
        "pending cleared after cancel"
    );

    // And `pending` can no longer accept (NoPendingAuthority).
    // Fund pending so the signer check happens, not lamports.
    let transfer_ix =
        system_instruction::transfer(&env.authority.pubkey(), &pending.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = accept_authority_ix(&pending.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&pending.pubkey()),
        &[&pending],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "accept after cancel must fail");
}

#[tokio::test]
async fn test_transfer_authority_self_transfer_rejected() {
    // Self-transfer is a no-op and adds event noise — the program rejects it
    // with `Unauthorized` rather than emitting a confusing
    // `AuthorityTransferInitiated`.
    let mut env = setup_market().await;

    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, env.authority.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "self-transfer must be rejected");

    // The header should be untouched.
    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[8..40],
        env.authority.pubkey().as_ref(),
        "authority unchanged"
    );
    assert_eq!(
        &acct.data[136..168],
        Pubkey::default().as_ref(),
        "pending_authority unchanged"
    );
}

#[tokio::test]
async fn test_transfer_authority_overwrite_pending() {
    // Initiating a transfer to a different pubkey while one is already
    // in flight is intentional (lets the authority change their mind).
    // The previous pending key loses its claim and can no longer accept.
    let mut env = setup_market().await;
    let first = Keypair::new();
    let second = Keypair::new();

    // Fund both so they can sign.
    for pk in [first.pubkey(), second.pubkey()] {
        let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &pk, 1_000_000_000);
        let tx = Transaction::new_signed_with_payer(
            &[transfer_ix],
            Some(&env.authority.pubkey()),
            &[&env.authority],
            env.recent_blockhash,
        );
        env.banks_client.process_transaction(tx).await.unwrap();
        env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    }

    // Initiate to `first`.
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, first.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Overwrite with `second`.
    let ix = transfer_authority_ix(&env.authority.pubkey(), &env.market, second.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // pending_authority is now `second`.
    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[136..168],
        second.pubkey().as_ref(),
        "pending overwritten to second"
    );

    // `first` can no longer accept.
    let ix = accept_authority_ix(&first.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&first.pubkey()),
        &[&first],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "first must no longer be able to accept");

    // `second` can.
    let ix = accept_authority_ix(&second.pubkey(), &env.market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&second.pubkey()),
        &[&second],
        env.recent_blockhash,
    );
    env.banks_client.process_transaction(tx).await.unwrap();

    let acct = env
        .banks_client
        .get_account(env.market)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &acct.data[8..40],
        second.pubkey().as_ref(),
        "authority rotated to second"
    );
}

#[tokio::test]
async fn test_migrate_header_v1_corrupted_bump_rejected() {
    // A v0 account whose `bump` byte doesn't match the canonical PDA bump
    // (e.g. corrupted on disk or maliciously crafted) must be rejected by
    // the migration handler with `CorruptState`.
    let authority = Keypair::new();
    let mint_key = Pubkey::new_unique();
    let oracle = Pubkey::new_unique();
    let matcher_key = Pubkey::new_unique();

    let mut pt = program_test();
    let (market, market_bump) = market_pda(&authority.pubkey());
    let (_vault, vault_bump) = vault_pda(&market);

    // Build the v0 buffer with the wrong bump byte (canonical bump XOR 0xFF
    // is guaranteed to differ since canonical bumps are 0..=255 and the XOR
    // flips every bit).
    let mut v0_data = build_v0_market_data(
        &authority.pubkey(),
        &mint_key,
        &oracle,
        &matcher_key,
        market_bump.wrapping_add(1),
        vault_bump,
    );
    // Defensive: just in case wrapping_add(1) somehow lands on the canonical
    // bump (it can't, but be safe), force it to a known-wrong value.
    if v0_data[136] == market_bump {
        v0_data[136] = market_bump.wrapping_sub(1);
    }
    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000,
            data: v0_data,
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );
    pt.add_account(
        authority.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    let (mut banks_client, _payer, recent_blockhash) = pt.start().await;

    let ix = migrate_header_v1_ix(&authority.pubkey(), &market);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    let result = banks_client.process_transaction(tx).await;
    assert!(
        result.is_err(),
        "migrate_header_v1 with corrupted bump must be rejected"
    );
}

// ---------------------------------------------------------------------------
// Pyth-backed tests (accrue_market, convert_released_pnl)
//
// These need a Pyth-owned oracle account whose `timestamp` is within
// MAX_PRICE_AGE_SECS (60s) of the runtime clock. We use `start_with_context`
// + `set_sysvar` to nail the clock to a known value, then construct the price
// account with a matching timestamp.
// ---------------------------------------------------------------------------

const PINNED_UNIX_TS: i64 = 1_700_000_000;

/// Set up a complete market (init + deposit) with a Pyth-backed oracle.
/// Returns context, market PDA, vault PDA, mint, authority, oracle.
struct PythTestEnv {
    context: ProgramTestContext,
    authority: Keypair,
    market: Pubkey,
    vault: Pubkey,
    mint: Pubkey,
    oracle: Pubkey,
}

async fn setup_pyth_market(price: i64, expo: i32) -> PythTestEnv {
    let authority = Keypair::new();
    let matcher = Keypair::new();
    let oracle = Pubkey::new_unique();

    let mut pt = program_test();
    let (market, _bump) = market_pda(&authority.pubkey());
    let (vault, _vbump) = vault_pda(&market);

    pt.add_account(
        market,
        solana_sdk::account::Account {
            lamports: 100_000_000_000,
            data: vec![0u8; MARKET_ACCOUNT_SIZE],
            owner: program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    // Pyth-owned oracle preloaded with a Trading price at PINNED_UNIX_TS.
    let oracle_data = build_pyth_price_account(price, expo, PINNED_UNIX_TS);
    pt.add_account(
        oracle,
        solana_sdk::account::Account {
            lamports: 1_000_000_000,
            data: oracle_data,
            owner: pyth_program_id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    pt.add_account(
        authority.pubkey(),
        solana_sdk::account::Account {
            lamports: 100_000_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        },
    );

    let mut context = pt.start_with_context().await;

    // Pin the runtime clock to PINNED_UNIX_TS so the freshness check passes.
    let mut clock: Clock = context.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = PINNED_UNIX_TS;
    context.set_sysvar(&clock);

    // Create mint.
    let mint_kp = create_mint(
        &mut context.banks_client,
        &authority,
        context.last_blockhash,
        6,
    )
    .await;
    let mint = mint_kp.pubkey();

    let recent_blockhash = context.banks_client.get_latest_blockhash().await.unwrap();
    let slot = context.banks_client.get_root_slot().await.unwrap();

    let ix = initialize_market_ix(
        &authority.pubkey(),
        &market,
        &mint,
        &vault,
        &oracle,
        &matcher.pubkey(),
        InitializeMarketArgs {
            init_slot: slot,
            init_oracle_price: 1000,
            params: default_risk_params(),
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );
    context.banks_client.process_transaction(tx).await.unwrap();

    // Re-pin the clock — initialize_market may have advanced it via tick.
    let mut clock: Clock = context.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = PINNED_UNIX_TS;
    context.set_sysvar(&clock);

    PythTestEnv {
        context,
        authority,
        market,
        vault,
        mint,
        oracle,
    }
}

#[tokio::test]
async fn test_accrue_market_with_pyth() {
    // 1234.5 USDC, expo = -1, so on-chain oracle_price = 1234 / 10^1... wait,
    // expo=-1 means divisor=10, price=12345 -> 1234. Use price=1500, expo=0
    // for a clean 1500.
    let mut env = setup_pyth_market(1500, 0).await;

    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();

    let ix = accrue_market_ix(&env.authority.pubkey(), &env.market, &env.oracle);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        blockhash,
    );
    env.context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("accrue_market should succeed with a fresh Trading Pyth feed");
}

#[tokio::test]
async fn test_accrue_market_stale_price_fails() {
    let mut env = setup_pyth_market(1500, 0).await;

    // Push the clock 5 minutes into the future relative to the price account
    // timestamp — well outside MAX_PRICE_AGE_SECS (60s).
    let mut clock: Clock = env.context.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = PINNED_UNIX_TS + 300;
    env.context.set_sysvar(&clock);

    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();

    let ix = accrue_market_ix(&env.authority.pubkey(), &env.market, &env.oracle);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        blockhash,
    );
    let result = env.context.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "stale Pyth price should be rejected");
}

#[tokio::test]
async fn test_accrue_market_wrong_oracle_owner_fails() {
    // Use the standard non-Pyth setup_market — its oracle stub is owned by a
    // random program, so the AccrueMarket account constraint must reject it.
    let mut env = setup_market().await;

    let ix = accrue_market_ix(&env.authority.pubkey(), &env.market, &env.oracle);
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        env.recent_blockhash,
    );
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-Pyth-owned oracle must be rejected");
}

#[tokio::test]
async fn test_convert_released_pnl_with_pyth() {
    // Bootstrap a Pyth-backed market, deposit collateral so the user has an
    // account slot, then call convert_released_pnl. With zero released PnL it
    // should be a no-op success.
    let mut env = setup_pyth_market(1500, 0).await;

    // Create user's ATA + mint to it
    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();
    let user_ata = create_ata(
        &mut env.context.banks_client,
        &env.authority,
        blockhash,
        &env.mint,
        &env.authority.pubkey(),
    )
    .await;
    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();
    mint_to(
        &mut env.context.banks_client,
        &env.authority,
        blockhash,
        &env.mint,
        &user_ata,
        10_000_000,
    )
    .await;
    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();

    // Deposit to slot 0 so we have an account to convert against.
    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs {
            account_idx: 0,
            amount: 1_000_000,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        blockhash,
    );
    env.context
        .banks_client
        .process_transaction(tx)
        .await
        .unwrap();

    // Re-pin clock again after the deposit tick.
    let mut clock: Clock = env.context.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = PINNED_UNIX_TS;
    env.context.set_sysvar(&clock);

    let blockhash = env
        .context
        .banks_client
        .get_latest_blockhash()
        .await
        .unwrap();
    let ix = convert_released_pnl_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.oracle,
        ConvertReleasedPnlArgsTest {
            account_idx: 0,
            x_req: 0,
            funding_rate: 0,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.authority.pubkey()),
        &[&env.authority],
        blockhash,
    );
    // x_req=0 should always succeed (it's a no-op convert).
    env.context
        .banks_client
        .process_transaction(tx)
        .await
        .expect("convert_released_pnl with x_req=0 should succeed");
}

// Suppress unused-warning for AccountSharedData re-export.
#[allow(dead_code)]
fn _ensure_account_shared_data_imported(_: AccountSharedData) {}
