//! A vault behind the Jupiter Lend adapter, on the real thing: the lending and
//! liquidity programs and the USDC market's accounts, as `fetch-fixtures.sh`
//! snapshot them from mainnet.

use crate::*;
use base64::Engine;
use mollusk_svm::program::create_program_account_loader_v3;
use solana_pubkey::pubkey;

pub const ADAPTER_ID: Pubkey = Pubkey::new_from_array(jupiter_lend_adapter::ID);
pub const LENDING: Pubkey = pubkey!("jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9");
pub const LIQUIDITY: Pubkey = pubkey!("jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC");
pub const ATA_PROGRAM: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const USDC: u64 = 1_000_000;
pub const DAY: i64 = 24 * 60 * 60;

pub fn fixture_json(name: &str) -> serde_json::Value {
    let path = format!("{}/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let file = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{path} is missing: run tests/fetch-fixtures.sh"));
    serde_json::from_str(&file).unwrap()
}

/// One account of the mainnet snapshot.
pub fn snapshot(name: &str) -> (Pubkey, Account) {
    let json = fixture_json(name);
    let account = &json["account"];
    let data = base64::engine::general_purpose::STANDARD
        .decode(account["data"][0].as_str().unwrap())
        .unwrap();
    (
        json["pubkey"].as_str().unwrap().parse().unwrap(),
        Account {
            lamports: account["lamports"].as_u64().unwrap(),
            data,
            owner: account["owner"].as_str().unwrap().parse().unwrap(),
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), TOKEN.as_ref(), mint.as_ref()],
        &ATA_PROGRAM,
    )
    .0
}

pub struct Market {
    pub f: Fixture,
    pub lending: Pubkey,
    pub f_token_mint: Pubkey,
    pub f_token_account: Pubkey,
    pub liquidity_vault: Pubkey,
    /// The lending program's permissionless `update_rate`, which stores the
    /// market's current exchange price.
    pub update_rate: Instruction,
}

/// A vault over mainnet USDC whose adapter is the Jupiter Lend adapter, on a
/// clock set to the moment of the snapshot.
pub fn market() -> Market {
    let mut f = Fixture::setup(ADAPTER_ID, TOKEN);
    f.mollusk.add_program(&ADAPTER_ID, "jupiter_lend_adapter");
    f.mollusk.add_program(&LENDING, "jupiter_lending");
    f.mollusk.add_program(&LIQUIDITY, "jupiter_liquidity");
    mollusk_svm_programs_token::associated_token::add_program(&mut f.mollusk);

    let clock = fixture_json("clock");
    f.mollusk.warp_to_slot(clock["slot"].as_u64().unwrap());
    f.mollusk.sysvars.clock.epoch = clock["epoch"].as_u64().unwrap();
    f.mollusk.sysvars.clock.unix_timestamp = clock["unix_timestamp"].as_i64().unwrap();

    let [mint, f_token_mint, lending, lending_admin, liquidity, rate_model, token_reserve, supply_position, liquidity_vault, rewards_rate_model] =
        [
            "mint",
            "f_token_mint",
            "lending",
            "lending_admin",
            "liquidity",
            "rate_model",
            "token_reserve",
            "supply_position",
            "liquidity_vault",
            "rewards_rate_model",
        ]
        .map(snapshot);

    // Stand the vault on the real mint, and on the strategy authority's
    // associated token accounts, as the lending program requires.
    f.rekey(f.asset_mint, mint.0);
    f.asset_mint = mint.0;
    *f.account_mut(&mint.0) = mint.1;
    let strategy_account = ata(&f.strategy_authority, &f.asset_mint);
    f.rekey(f.strategy_account, strategy_account);
    f.strategy_account = strategy_account;
    let f_token_account = ata(&f.strategy_authority, &f_token_mint.0);
    f.set_balance(f.user_assets, 10_000 * USDC);

    f.adapter_own = vec![
        AccountMeta::new(f_token_account, false),
        AccountMeta::new(lending.0, false),
        AccountMeta::new_readonly(lending_admin.0, false),
        AccountMeta::new(f_token_mint.0, false),
        AccountMeta::new(token_reserve.0, false),
        AccountMeta::new(supply_position.0, false),
        AccountMeta::new_readonly(rate_model.0, false),
        AccountMeta::new(liquidity_vault.0, false),
        AccountMeta::new(liquidity.0, false),
        AccountMeta::new_readonly(LIQUIDITY, false),
        AccountMeta::new_readonly(rewards_rate_model.0, false),
        AccountMeta::new_readonly(ATA_PROGRAM, false),
        AccountMeta::new_readonly(SYSTEM, false),
        AccountMeta::new_readonly(LENDING, false),
    ];
    let update_rate = Instruction {
        program_id: LENDING,
        accounts: vec![
            AccountMeta::new(lending.0, false),
            AccountMeta::new_readonly(f.asset_mint, false),
            AccountMeta::new_readonly(f_token_mint.0, false),
            AccountMeta::new_readonly(token_reserve.0, false),
            AccountMeta::new_readonly(rewards_rate_model.0, false),
        ],
        data: solana_sha256_hasher::hash(b"global:update_rate").to_bytes()[..8].to_vec(),
    };

    let mut market = Market {
        lending: lending.0,
        f_token_mint: f_token_mint.0,
        f_token_account,
        liquidity_vault: liquidity_vault.0,
        update_rate,
        f,
    };
    market.f.accounts.extend([
        f_token_mint.clone(),
        lending,
        lending_admin,
        liquidity,
        rate_model,
        token_reserve,
        supply_position,
        liquidity_vault,
        rewards_rate_model,
        (
            f_token_account,
            token_account(&TOKEN, &f_token_mint.0, &market.f.strategy_authority, 0),
        ),
        (ADAPTER_ID, create_program_account_loader_v3(&ADAPTER_ID)),
        (LENDING, create_program_account_loader_v3(&LENDING)),
        (LIQUIDITY, create_program_account_loader_v3(&LIQUIDITY)),
        mollusk_svm_programs_token::associated_token::keyed_account(),
    ]);
    market.f.create();
    market
}
