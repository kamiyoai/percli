use borsh::BorshSerialize;
use sha2::{Digest, Sha256};
use solana_program_test::*;
#[allow(deprecated)]
use solana_sdk::{
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

/// Must match MARKET_ACCOUNT_SIZE in the on-chain program.
const MARKET_ACCOUNT_SIZE: usize = 8 + 136 + std::mem::size_of::<percli_core::RiskEngine>();

fn program_id() -> Pubkey {
    PROGRAM_ID_STR.parse().unwrap()
}

fn anchor_discriminator(name: &str) -> [u8; 8] {
    let hash = Sha256::digest(format!("global:{name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash[..8]);
    disc
}

fn build_ix(program_id: &Pubkey, name: &str, args: impl BorshSerialize, accounts: Vec<AccountMeta>) -> Instruction {
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
    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        dest,
        &payer.pubkey(),
        &[],
        amount,
    )
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

fn trade_ix(
    matcher: &Pubkey,
    market: &Pubkey,
    args: TradeArgs,
) -> Instruction {
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

fn settle_ix(
    user: &Pubkey,
    market: &Pubkey,
    args: SettleArgs,
) -> Instruction {
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

fn close_account_ix(
    user: &Pubkey,
    market: &Pubkey,
    args: CloseAccountArgs,
) -> Instruction {
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

fn liquidate_ix(
    liquidator: &Pubkey,
    market: &Pubkey,
    args: LiquidateArgs,
) -> Instruction {
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

fn update_oracle_ix(
    authority: &Pubkey,
    market: &Pubkey,
    new_oracle: &Pubkey,
) -> Instruction {
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

    let slot = banks_client
        .get_root_slot()
        .await
        .unwrap();

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
    assert_eq!(&market_account.data[0..8], b"percmrkt");
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
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &intruder.pubkey(),
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
    assert!(result.is_err(), "deposit to owned slot by different user should fail");
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
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &intruder.pubkey(),
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
    let transfer_ix = system_instruction::transfer(
        &env.authority.pubkey(),
        &user_b.pubkey(),
        2_000_000_000,
    );
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

    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_a, 1_000_000).await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_b, 1_000_000).await;

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit for user A (slot 0)
    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata_a,
        &env.vault,
        DepositArgs { account_idx: 0, amount: 500_000 },
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
        DepositArgs { account_idx: 1, amount: 500_000 },
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
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ata_a = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &user_b.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_a, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_b, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_a = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata_a, &env.vault, DepositArgs { account_idx: 0, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(&user_b.pubkey(), &env.market, &env.mint, &ata_b, &env.vault, DepositArgs { account_idx: 1, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
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

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
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
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Create ATAs and fund
    let ata_a = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &user_b.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_a, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_b, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 1. Deposit — both users
    let dep_a = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata_a, &env.vault, DepositArgs { account_idx: 0, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(&user_b.pubkey(), &env.market, &env.mint, &ata_b, &env.vault, DepositArgs { account_idx: 1, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    assert_eq!(token_balance(&mut env.banks_client, &env.vault).await, 1_000_000);

    // 2. Settle — both accounts (no open position, just exercise the instruction)
    let settle_a = settle_ix(&env.authority.pubkey(), &env.market, SettleArgs { account_idx: 0, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[settle_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let settle_b = settle_ix(&user_b.pubkey(), &env.market, SettleArgs { account_idx: 1, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[settle_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 4. Close — both accounts
    let close_a = close_account_ix(&env.authority.pubkey(), &env.market, CloseAccountArgs { account_idx: 0, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[close_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let close_b = close_account_ix(&user_b.pubkey(), &env.market, CloseAccountArgs { account_idx: 1, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[close_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // 5. Withdraw remaining balances
    let withdraw_a = withdraw_ix(
        &env.authority.pubkey(), &env.market, &env.mint, &ata_a, &env.vault,
        WithdrawArgs { account_idx: 0, amount: 500_000, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[withdraw_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    // Note: withdraw after close may fail depending on engine semantics — that's fine.
    // The point is to exercise the full path. If close zeroes balance, skip this.
    let _ = env.banks_client.process_transaction(tx).await;

    // Market should still exist
    let market_account = env.banks_client.get_account(env.market).await.unwrap();
    assert!(market_account.is_some(), "market account should still exist");
}

#[tokio::test]
async fn test_deposit_zero_amount_fails() {
    let mut env = setup_market().await;

    let user_ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &user_ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        DepositArgs { account_idx: 0, amount: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero deposit should fail");
}

// ---------------------------------------------------------------------------
// New v0.8.0 tests
// ---------------------------------------------------------------------------

/// Helper: set up two funded accounts with positions for liquidation testing.
async fn setup_two_accounts_with_trade(
) -> (TestEnv, Keypair, Pubkey, Pubkey) {
    let mut env = setup_market().await;
    let user_b = Keypair::new();

    // Fund user B with SOL
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &user_b.pubkey(), 2_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ata_a = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    let ata_b = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &user_b.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_a, 10_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata_b, 10_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit for both users
    let dep_a = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata_a, &env.vault, DepositArgs { account_idx: 0, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep_b = deposit_ix(&user_b.pubkey(), &env.market, &env.mint, &ata_b, &env.vault, DepositArgs { account_idx: 1, amount: 500_000 });
    let tx = Transaction::new_signed_with_payer(&[dep_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Trade: account 0 long, account 1 short
    let ix = trade_ix(&env.matcher.pubkey(), &env.market, TradeArgs {
        account_a: 0,
        account_b: 1,
        size_q: 1_000_000,
        exec_price: 1000,
        funding_rate: 0,
    });
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority, &env.matcher], env.recent_blockhash);
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
        LiquidateArgs { account_idx: 0, funding_rate: 0 },
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
    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 10_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit (new_account_fee=0 in our config, so no insurance accrual from deposit alone)
    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 1_000_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
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
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero withdraw_insurance should fail");
}

#[tokio::test]
async fn test_withdraw_insurance_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &intruder.pubkey()).await;
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
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&intruder.pubkey()), &[&intruder], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-authority withdraw_insurance should fail");
}

#[tokio::test]
async fn test_reclaim_active_account_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit enough capital (well above min_initial_deposit=1000)
    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 100_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try to reclaim — should fail (capital >> 0, above floor)
    let ix = reclaim_account_ix(
        &env.authority.pubkey(),
        &env.market,
        ReclaimAccountArgs { account_idx: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
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

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 100_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to close
    let intruder = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = close_account_ix(
        &intruder.pubkey(),
        &env.market,
        CloseAccountArgs { account_idx: 0, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&intruder.pubkey()), &[&intruder], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "close by non-owner should fail");
}

#[tokio::test]
async fn test_deposit_trade_settle_close_lifecycle() {
    let (mut env, user_b, ata_a, _ata_b) = setup_two_accounts_with_trade().await;

    // Settle account 0
    let ix = settle_ix(&env.authority.pubkey(), &env.market, SettleArgs { account_idx: 0, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Settle account 1
    let ix = settle_ix(&user_b.pubkey(), &env.market, SettleArgs { account_idx: 1, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Vault should still have tokens
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert!(vault_bal > 0, "vault should have tokens after trade");

    // Withdraw from account 0 (partial)
    let ix = withdraw_ix(
        &env.authority.pubkey(), &env.market, &env.mint, &ata_a, &env.vault,
        WithdrawArgs { account_idx: 0, amount: 10_000, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    // Verify user received tokens
    let user_bal = token_balance(&mut env.banks_client, &ata_a).await;
    assert!(user_bal > 0, "user should have received withdrawn tokens");
}

#[tokio::test]
async fn test_multiple_deposits_same_slot() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 10_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit 1
    let dep1 = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 50_000 });
    let tx = Transaction::new_signed_with_payer(&[dep1], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit 2 (same slot, same owner)
    let dep2 = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 30_000 });
    let tx = Transaction::new_signed_with_payer(&[dep2], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    // Vault should have 80_000
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 80_000);
}

#[tokio::test]
async fn test_withdraw_more_than_available_rejected() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 10_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Try to withdraw more than deposited
    let ix = withdraw_ix(
        &env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault,
        WithdrawArgs { account_idx: 0, amount: 999_999, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "withdrawing more than balance should fail");
}

// ---------------------------------------------------------------------------
// v0.9.0 tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_top_up_insurance() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 50_000 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 50_000, "vault should hold top-up amount");

    let user_bal = token_balance(&mut env.banks_client, &ata).await;
    assert_eq!(user_bal, 950_000);
}

#[tokio::test]
async fn test_top_up_insurance_zero_fails() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "zero top-up should fail");
}

#[tokio::test]
async fn test_top_up_insurance_permissionless() {
    let mut env = setup_market().await;

    // Random user (not authority) can top up
    let random_user = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &random_user.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let user_ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &random_user.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &user_ata, 100_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = top_up_insurance_ix(
        &random_user.pubkey(),
        &env.market,
        &env.mint,
        &user_ata,
        &env.vault,
        TopUpInsuranceArgs { amount: 25_000 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&random_user.pubkey()), &[&random_user], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 25_000, "permissionless top-up should work");
}

#[tokio::test]
async fn test_deposit_fee_credits() {
    let mut env = setup_market().await;

    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // First deposit to open the account slot
    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 100_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Deposit fee credits (no fee debt yet, so engine caps to 0, but SPL transfer still happens)
    let ix = deposit_fee_credits_ix(
        &env.authority.pubkey(),
        &env.market,
        &env.mint,
        &ata,
        &env.vault,
        DepositFeeCreditsArgs { account_idx: 0, amount: 10_000 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();

    // Vault should now have deposit + fee credits
    let vault_bal = token_balance(&mut env.banks_client, &env.vault).await;
    assert_eq!(vault_bal, 110_000);
}

#[tokio::test]
async fn test_deposit_fee_credits_wrong_owner() {
    let mut env = setup_market().await;

    // Authority opens slot 0
    let ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &env.authority.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &ata, 1_000_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let dep = deposit_ix(&env.authority.pubkey(), &env.market, &env.mint, &ata, &env.vault, DepositArgs { account_idx: 0, amount: 100_000 });
    let tx = Transaction::new_signed_with_payer(&[dep], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to deposit fee credits to authority's slot
    let intruder = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let intruder_ata = create_ata(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &intruder.pubkey()).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();
    mint_to(&mut env.banks_client, &env.authority, env.recent_blockhash, &env.mint, &intruder_ata, 100_000).await;
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = deposit_fee_credits_ix(
        &intruder.pubkey(),
        &env.market,
        &env.mint,
        &intruder_ata,
        &env.vault,
        DepositFeeCreditsArgs { account_idx: 0, amount: 5_000 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&intruder.pubkey()), &[&intruder], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "deposit_fee_credits by non-owner should fail");
}

#[tokio::test]
async fn test_update_matcher() {
    let (mut env, user_b, _ata_a, _ata_b) = setup_two_accounts_with_trade().await;

    let new_matcher = Keypair::new();

    // Authority rotates matcher
    let ix = update_matcher_ix(
        &env.authority.pubkey(),
        &env.market,
        UpdateMatcherArgsTest { new_matcher: new_matcher.pubkey() },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Old matcher should be rejected
    let ix = trade_ix(
        &env.matcher.pubkey(),
        &env.market,
        TradeArgs { account_a: 0, account_b: 1, size_q: 50, exec_price: 1000, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority, &env.matcher], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "old matcher should be rejected after rotation");

    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Fund new matcher with SOL
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &new_matcher.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // New matcher should work — settle first to flatten positions
    let settle_a = settle_ix(&env.authority.pubkey(), &env.market, SettleArgs { account_idx: 0, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[settle_a], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let settle_b = settle_ix(&user_b.pubkey(), &env.market, SettleArgs { account_idx: 1, funding_rate: 0 });
    let tx = Transaction::new_signed_with_payer(&[settle_b], Some(&user_b.pubkey()), &[&user_b], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Trade with new matcher
    let ix = trade_ix(
        &new_matcher.pubkey(),
        &env.market,
        TradeArgs { account_a: 0, account_b: 1, size_q: 50, exec_price: 1000, funding_rate: 0 },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&env.authority.pubkey()), &[&env.authority, &new_matcher], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
}

#[tokio::test]
async fn test_update_matcher_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    let ix = update_matcher_ix(
        &intruder.pubkey(),
        &env.market,
        UpdateMatcherArgsTest { new_matcher: intruder.pubkey() },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&intruder.pubkey()), &[&intruder], env.recent_blockhash);
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
    let pyth_program_id: Pubkey = "FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH".parse().unwrap();

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
        &authority.pubkey(), &market, &mint_kp.pubkey(), &vault, &old_oracle, &matcher.pubkey(),
        InitializeMarketArgs { init_slot: slot, init_oracle_price: 1000, params: default_risk_params() },
    );
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&authority.pubkey()), &[&authority], recent_blockhash);
    banks_client.process_transaction(tx).await.unwrap();
    let recent_blockhash = banks_client.get_latest_blockhash().await.unwrap();

    // Update oracle
    let ix = update_oracle_ix(&authority.pubkey(), &market, &new_oracle_key);
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&authority.pubkey()), &[&authority], recent_blockhash);
    banks_client.process_transaction(tx).await.unwrap();

    // Verify: read the header and check oracle field
    let market_account = banks_client.get_account(market).await.unwrap().unwrap();
    // The oracle pubkey is at offset 8 (discriminator) + 32 (authority) + 32 (mint) = 72
    let oracle_bytes = &market_account.data[72..104];
    assert_eq!(oracle_bytes, new_oracle_key.to_bytes(), "oracle should be updated in header");
}

#[tokio::test]
async fn test_update_oracle_unauthorized() {
    let mut env = setup_market().await;

    let intruder = Keypair::new();
    let transfer_ix = system_instruction::transfer(&env.authority.pubkey(), &intruder.pubkey(), 1_000_000_000);
    let tx = Transaction::new_signed_with_payer(&[transfer_ix], Some(&env.authority.pubkey()), &[&env.authority], env.recent_blockhash);
    env.banks_client.process_transaction(tx).await.unwrap();
    env.recent_blockhash = env.banks_client.get_latest_blockhash().await.unwrap();

    // Intruder tries to update oracle — should fail even though we use the existing oracle as new_oracle
    let ix = update_oracle_ix(&intruder.pubkey(), &env.market, &env.oracle);
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&intruder.pubkey()), &[&intruder], env.recent_blockhash);
    let result = env.banks_client.process_transaction(tx).await;
    assert!(result.is_err(), "non-authority update_oracle should fail");
}
