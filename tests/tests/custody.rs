//! The custody adapter's own rules, under a real vault: funds go out to one
//! account, the book is units at an oracle's price, and funds come back by a
//! plain transfer.

use custody_adapter::constants::*;
use custody_adapter::errors::CustodyError;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;
use vault_core::errors::VaultError;
use vault_tests::custody::*;
use vault_tests::*;

fn custom(error: CustodyError) -> Option<u32> {
    Some(error as u32)
}

#[test]
fn the_book_is_units_at_the_oracles_price() {
    let mut c = Custodied::setup(TOKEN);
    c.f.create();

    // Only the vault's owner opens the book, and only once.
    let ix = c.initialize_ix(c.f.stranger);
    assert_eq!(
        custom_error(&c.f.run(&ix)),
        custom(CustodyError::InvalidOwner)
    );
    c.f.ok(&c.initialize_ix(c.f.owner));
    let ix = c.initialize_ix(c.f.owner);
    assert_eq!(
        custom_error(&c.f.run(&ix)),
        custom(CustodyError::InvalidCustody)
    );

    // The manager decides when funds leave, not where they go.
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    let elsewhere = Pubkey::new_unique();
    c.f.accounts.push((
        elsewhere,
        token_account(&TOKEN, &c.f.asset_mint, &c.f.manager, 0),
    ));
    let mut redirected = c.f.allocate_ix(700);
    redirected.accounts.last_mut().unwrap().pubkey = elsewhere;
    assert_eq!(
        custom_error(&c.f.run(&redirected)),
        custom(CustodyError::InvalidTokenAccount)
    );

    // An empty book starts at par: 700 out buys 700 units.
    c.f.ok(&c.f.allocate_ix(700));
    assert_eq!(c.f.balance(&c.destination), 700);
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (300, 700));

    // The price is what the oracle signed. Anyone can deliver it; nobody can
    // forge it or deliver one that has expired.
    let par = PRICE_SCALE;
    let forged = c.price_ix(&SigningKey::from_bytes(&[9; 32]), 2 * par, 60);
    assert_eq!(
        custom_error(&c.f.run(&forged)),
        custom(CustodyError::InvalidReport)
    );
    let expired = c.price_ix(&oracle(), par, -1);
    assert_eq!(
        custom_error(&c.f.run(&expired)),
        custom(CustodyError::StaleReport)
    );

    // A loss: 0.9 a unit makes 700 units worth 630.
    let down = c.price_ix(&oracle(), par / 10 * 9, 60);
    c.f.ok(&down);
    assert_eq!(c.f.state().debt(), 630);

    // Anyone can reprice from the stored price while it lasts, and present
    // the same report again; an older report cannot be played back over it.
    c.f.ok(&c.f.simulate_ix());
    c.f.ok(&down);
    let older = c.price_ix(&oracle(), par, 59);
    assert_eq!(
        custom_error(&c.f.run(&older)),
        custom(CustodyError::StaleReport)
    );

    // A price signed before a flow is still right after it. 300 more go out
    // at 0.9 and buy 333 units; nothing needs re-signing, and the book is
    // worth 1_033 × 0.9 = 929, one unit of rounding under 630 + 300.
    c.f.ok(&c.f.allocate_ix(300));
    c.f.ok(&c.f.simulate_ix());
    assert_eq!(c.f.state().debt(), 929);

    // Once the price expires the adapter stops answering, so the vault goes
    // stale and prices nothing until the oracle signs again.
    c.f.advance(1, 61);
    let ix = c.f.simulate_ix();
    assert_eq!(
        custom_error(&c.f.run(&ix)),
        custom(CustodyError::StalePrice)
    );
    c.f.fails(&c.f.deposit_ix(10, 0), VaultError::StaleReport);

    // A new price is adopted as signed. How fast a gain then reaches the
    // share price is the vault's business, not the adapter's.
    c.f.ok(&c.price_ix(&oracle(), par, 60));
    assert_eq!(c.f.state().debt(), 1_033);
}

#[test]
fn funds_come_back_by_a_plain_transfer() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(700));
    c.f.ok(&c.price_ix(&oracle(), PRICE_SCALE, 3_600));

    // Nothing sent back, nothing to bring home.
    c.f.ok(&c.f.deallocate_ix(500));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (300, 700));

    // The custodian sends 300 to the return account. Until it is settled it
    // still counts as the units it will redeem, so the value does not move.
    c.send_back(300);
    c.f.ok(&c.f.simulate_ix());
    assert_eq!(c.f.state().debt(), 700);

    // A deallocation settles it against units and brings it to idle.
    c.f.ok(&c.f.deallocate_ix(500));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (600, 400));
    assert_eq!(c.f.balance(&c.return_account), 0);
    c.f.ok(&c.f.simulate_ix());
    assert_eq!(c.f.state().debt(), 400);

    // A redemption pulls returned funds itself, without waiting for the
    // manager: 1_000 shares are owed 1_000, idle holds 600, 400 came back.
    c.send_back(400);
    c.f.ok(&c.f.request_redeem_ix(1_000, 1));
    c.f.ok(&c.f.fulfil_ix(1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE);
    assert_eq!(c.f.ticket_shares(1), None);
    assert_eq!(c.f.state().total_assets().unwrap(), 0);

    // The vault's rules are unchanged: only its manager allocates.
    let mut ix = c.f.allocate_ix(1);
    ix.accounts[0].pubkey = c.f.stranger;
    c.f.fails(&ix, VaultError::InvalidAuthority);
}

#[test]
fn more_coming_back_than_the_book_holds_is_a_gain() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));
    c.f.ok(&c.price_ix(&oracle(), PRICE_SCALE, 10 * YEAR));

    // The custodian returns 1_100 for 1_000 units. The book is empty after
    // the first 1_000; the vault takes back its principal, and the other 100
    // waits in the strategy account to be counted as a gain, at the rate.
    c.send_back(1_100);
    c.f.ok(&c.f.deallocate_ix(1_100));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (1_000, 0));
    assert_eq!(c.f.balance(&c.f.strategy_account), 100);
}

#[test]
fn a_new_oracle_waits_out_the_vaults_timelock() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));
    let next = SigningKey::from_bytes(&[12; 32]);

    let ix = c.set_oracle_ix(c.f.stranger, key(&next));
    assert_eq!(
        custom_error(&c.f.run(&ix)),
        custom(CustodyError::InvalidOwner)
    );
    c.f.ok(&c.set_oracle_ix(c.f.owner, key(&next)));

    // Named is not pricing yet: the old oracle still is.
    let early = c.price_ix(&next, PRICE_SCALE, 60);
    assert_eq!(
        custom_error(&c.f.run(&early)),
        custom(CustodyError::InvalidReport)
    );
    c.f.ok(&c.price_ix(&oracle(), PRICE_SCALE, 60));

    // After the vault's timelock the new one takes over, and the old one is out.
    c.f.advance(1, c.f.timelock as i64);
    c.f.ok(&c.price_ix(&next, PRICE_SCALE, 60));
    let old = c.price_ix(&oracle(), PRICE_SCALE, 120);
    assert_eq!(
        custom_error(&c.f.run(&old)),
        custom(CustodyError::InvalidReport)
    );
}

#[test]
fn own_discriminators_are_namespaced_hashes() {
    for (name, discriminator) in [("initialize", INITIALIZE), ("set_oracle", SET_ORACLE)] {
        let hash = solana_sha256_hasher::hash(format!("solana-vault-custody:{name}").as_bytes());
        assert_eq!(hash.to_bytes()[..8], discriminator);
    }
}

#[test]
fn only_the_vault_moves_funds_through_the_adapter() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(700));
    c.f.ok(&c.price_ix(&oracle(), PRICE_SCALE, 3_600));
    c.send_back(300);

    // A stranger calls the adapter's `withdraw` directly, naming the real
    // strategy authority but not signing for it, and their own token account
    // as the place to send the returned funds. Only the vault can sign for
    // the strategy authority, so nothing moves and the book is untouched.
    let thief = Pubkey::new_unique();
    c.f.accounts.push((
        thief,
        token_account(&TOKEN, &c.f.asset_mint, &c.f.stranger, 0),
    ));
    let mut accounts = c.f.adapter_accounts("withdraw");
    accounts.remove(4); // the adapter program itself is not one of its accounts
    accounts[1].pubkey = thief;
    let withdraw = Instruction {
        program_id: CUSTODY_ID,
        accounts,
        data: [
            &vault_core::constants::ADAPTER_WITHDRAW[..],
            &u64::MAX.to_le_bytes(),
        ]
        .concat(),
    };
    assert_eq!(
        custom_error(&c.f.run(&withdraw)),
        custom(CustodyError::NotSigner)
    );
    assert_eq!(c.f.balance(&c.return_account), 300);
    assert_eq!(c.units(), 700);
}

#[test]
fn a_report_cannot_be_replayed_over_a_changed_book() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));

    // The oracle marks the book at 0.5. The custodian returns what is left
    // and the vault takes it home, which empties the book.
    let half = c.price_ix(&oracle(), PRICE_SCALE / 2, 3_600);
    c.f.ok(&half);
    assert_eq!(c.f.state().debt(), 500);
    // The oracle also signs a later report for this book, not yet delivered.
    let pending = c.price_ix(&oracle(), PRICE_SCALE / 2, 7_200);
    c.send_back(500);
    c.f.ok(&c.f.deallocate_ix(500));
    assert_eq!(c.units(), 0);

    // New funds go out, at par, into a fresh book. The old 0.5 report is
    // still unexpired, but it priced a book that no longer exists.
    c.f.ok(&c.f.allocate_ix(500));
    assert_eq!(
        custom_error(&c.f.run(&half)),
        custom(CustodyError::StaleReport)
    );
    assert_eq!(c.f.state().debt(), 500);
    // Neither does the one signed for the old book and delivered late: a
    // report names its book, and this is a new one.
    assert_eq!(
        custom_error(&c.f.run(&pending)),
        custom(CustodyError::InvalidReport)
    );
    assert_eq!(c.f.state().debt(), 500);

    // Two reports with the same expiry cannot be played against each other.
    let par = c.price_ix(&oracle(), PRICE_SCALE, 7_200);
    let down = c.price_ix(&oracle(), PRICE_SCALE / 10 * 9, 7_200);
    c.f.ok(&par);
    assert_eq!(
        custom_error(&c.f.run(&down)),
        custom(CustodyError::StaleReport)
    );
    c.f.ok(&par);
}

#[test]
fn a_vault_has_exactly_one_custody() {
    let mut c = Custodied::new();

    // The book is at the canonical address for the vault's strategy
    // authority. No other address can be opened as a second one.
    let other = (0..=255u8)
        .rev()
        .filter_map(|bump| {
            Pubkey::create_program_address(
                &[CUSTODY_SEED, c.f.strategy_authority.as_ref(), &[bump]],
                &CUSTODY_ID,
            )
            .ok()
        })
        .find(|address| *address != c.custody)
        .unwrap();
    let return_account = Pubkey::new_unique();
    c.f.accounts.push((
        return_account,
        token_account(&TOKEN, &c.f.asset_mint, &other, 0),
    ));
    let mut ix = c.initialize_ix(c.f.owner);
    ix.accounts[3].pubkey = other;
    ix.accounts[5].pubkey = return_account;
    assert_eq!(
        custom_error(&c.f.run(&ix)),
        custom(CustodyError::InvalidCustody)
    );
}
