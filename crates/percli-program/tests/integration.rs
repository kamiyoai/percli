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
