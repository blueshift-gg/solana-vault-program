//! The vault on Jupiter Lend itself: the real lending and liquidity programs
//! and a mainnet snapshot of the USDC market, with the vault program and the
//! adapter on top.

use jupiter_lend_adapter::constants::*;
use jupiter_lend_adapter::errors::AdapterError;
use solana_pubkey::Pubkey;
use vault_core::errors::VaultError;
use vault_tests::jupiter::*;
use vault_tests::*;

#[test]
fn a_vault_earns_through_jupiter_lend_and_exits_in_full() {
    let Market {
        mut f,
        f_token_account,
        liquidity_vault,
        update_rate,
        ..
    } = market();
    let pool_before = f.balance(&liquidity_vault);

    // 600 USDC go through the adapter into the market and come back as fTokens.
    f.ok(&f.deposit_ix(1_000 * USDC, 0));
    f.ok(&f.allocate_ix(600 * USDC));
    assert_eq!(
        (f.state().idle(), f.state().debt()),
        (400 * USDC, 600 * USDC)
    );
    assert_eq!(f.balance(&liquidity_vault), pool_before + 600 * USDC);
    assert_eq!(f.balance(&f.strategy_account), 0);
    assert!(f.balance(&f_token_account) > 0);

    // Valued right away, the position is worth what went in, less the
    // rounding of two conversions.
    f.reprice();
    assert!(600 * USDC - f.state().debt() <= 2);

    // A month of interest. The stored price is stale until the market is
    // used, so the first report sees nothing new; once anyone runs the
    // lending program's `update_rate`, the next report does.
    f.advance(1, 30 * DAY);
    f.reprice();
    assert!(f.state().debt() <= 600 * USDC);
    f.ok(&update_rate);
    f.reprice();
    let earned = f.state().debt() - 600 * USDC;
    assert!(earned > 0 && earned < 60 * USDC, "earned {earned}");

    // The manager takes 100 USDC back out.
    f.ok(&f.deallocate_ix(100 * USDC));
    assert_eq!(f.state().idle(), 500 * USDC);

    // The one holder leaves. Idle covers half; the fulfilment pulls the rest
    // straight out of the market, and the holder walks away with the interest.
    let shares = f.balance(&f.user_shares);
    f.ok(&f.request_redeem_ix(shares, 1));
    f.ok(&f.fulfil_ix(1));
    assert_eq!(f.ticket_shares(1), None);
    let paid = f.balance(&f.user_assets) - (10_000 - 1_000) * USDC;
    assert!(
        paid > 1_000 * USDC && paid <= 1_000 * USDC + earned,
        "paid {paid}"
    );
    assert_eq!(f.state().idle(), 0);
    assert!(f.state().debt() <= 2);
}

#[test]
fn only_the_real_market_and_position_are_valued() {
    let Market {
        mut f,
        lending,
        f_token_mint,
        ..
    } = market();
    f.ok(&f.deposit_ix(1_000 * USDC, 0));
    f.ok(&f.allocate_ix(600 * USDC));
    const INVALID_LENDING: u32 = AdapterError::InvalidLending as u32;
    const INVALID_F_TOKEN_ACCOUNT: u32 = AdapterError::InvalidFTokenAccount as u32;

    // Anyone can open another fToken account for the strategy authority and
    // leave dust in it. Passing it instead would report the vault nearly
    // empty, so only the associated token account is accepted.
    let dust = Pubkey::new_unique();
    f.accounts.push((
        dust,
        token_account(&TOKEN, &f_token_mint, &f.strategy_authority, 1),
    ));
    let mut ix = f.simulate_ix();
    ix.accounts[9].pubkey = dust;
    assert_eq!(custom_error(&f.run(&ix)), Some(INVALID_F_TOKEN_ACCOUNT));

    // A copy of the market under another owner, with any price written into
    // it, is not the market.
    let forged = Pubkey::new_unique();
    let mut account = f.account(&lending).clone();
    account.owner = ADAPTER_ID;
    f.accounts.push((forged, account));
    let mut ix = f.simulate_ix();
    ix.accounts[10].pubkey = forged;
    assert_eq!(custom_error(&f.run(&ix)), Some(INVALID_LENDING));

    // Neither is another program dressed as the lending program.
    let mut ix = f.allocate_ix(USDC);
    let last = ix.accounts.len() - 1;
    ix.accounts[last].pubkey = LIQUIDITY;
    assert!(f.run(&ix).program_result.is_err());

    // The vault's own rules still hold on top.
    let mut ix = f.allocate_ix(USDC);
    ix.accounts[0].pubkey = f.stranger;
    f.fails(&ix, VaultError::InvalidAuthority);
}

#[test]
fn constants_match_the_lending_program() {
    assert_eq!(LENDING_PROGRAM, LENDING.to_bytes());
    for (name, discriminator) in [
        ("global:deposit", LENDING_DEPOSIT),
        ("global:withdraw", LENDING_WITHDRAW),
        ("account:Lending", LENDING_ACCOUNT),
    ] {
        let hash = solana_sha256_hasher::hash(name.as_bytes());
        assert_eq!(hash.to_bytes()[..8], discriminator);
    }

    // The layout offsets, read from the mainnet account.
    let (_, lending) = snapshot("lending");
    let (mint, _) = snapshot("mint");
    let (f_token_mint, _) = snapshot("f_token_mint");
    assert_eq!(lending.owner, LENDING);
    assert_eq!(lending.data[..8], LENDING_ACCOUNT);
    assert_eq!(
        lending.data[LENDING_MINT..LENDING_MINT + 32],
        mint.to_bytes()
    );
    assert_eq!(
        lending.data[LENDING_F_TOKEN_MINT..LENDING_F_TOKEN_MINT + 32],
        f_token_mint.to_bytes()
    );
    // One fToken has been worth more than one USDC since the market opened,
    // and not yet two.
    let price = u64::from_le_bytes(
        lending.data[LENDING_TOKEN_EXCHANGE_PRICE..LENDING_TOKEN_EXCHANGE_PRICE + 8]
            .try_into()
            .unwrap(),
    );
    assert!((1..2).contains(&(price as u128 / EXCHANGE_PRICES_PRECISION)));
}
