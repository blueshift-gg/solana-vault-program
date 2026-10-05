//! The vault end to end, behind the custody adapter: the vault program, the
//! adapter and the token programs run together, and the test plays the
//! people around them. `c.value(x)` is the oracle signing a price that makes
//! the off-chain position worth `x`; `c.send_back(x)` is the custodian
//! returning funds with a plain transfer.

use custody_adapter::constants::PRICE_SCALE;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;
use vault_core::constants::{
    PERMIT_DEPOSIT, PERMIT_FULFIL, VIEW_CONVERT_TO_ASSETS, VIEW_CONVERT_TO_SHARES, VIEW_MAX_DEPOSIT,
};
use vault_core::errors::VaultError;
use vault_tests::custody::*;
use vault_tests::*;

/// The same instruction without the adapter accounts: idle funds only.
fn idle_only(f: &Fixture, mut ix: Instruction) -> Instruction {
    ix.accounts
        .truncate(ix.accounts.len() - f.adapter_accounts().len());
    ix
}

/// A custody vault with its configuration changed before it is created.
fn vault_with(configure: impl FnOnce(&mut ConfigArgs)) -> Custodied {
    let mut c = Custodied::setup(TOKEN);
    configure(&mut c.f.config);
    c.open();
    c
}

// ---- Lifecycle ----

#[test]
fn vault_lifecycle() {
    let mut c = Custodied::new();

    // An empty vault prices 1:1.
    c.f.ok(&c.f.deposit_ix(1_000, 1_000));
    assert_eq!(c.f.balance(&c.f.user_shares), 1_000);
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (1_000, 0));

    // Allocating moves principal from idle to debt, through the adapter and
    // out to the custodian.
    c.f.ok(&c.f.allocate_ix(600));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (400, 600));
    assert_eq!(c.f.balance(&c.destination), 600);
    assert_eq!(c.f.balance(&c.f.strategy_account), 0);

    // Half a year later the position is worth 660. Until the vault is
    // repriced, the stored value is stale and nothing can be priced.
    c.f.advance(10, YEAR / 2);
    c.f.fails(&c.f.deposit_ix(100, 0), VaultError::StaleReport);
    let result = c.value(660);
    assert_eq!(result.return_data, 660u64.to_le_bytes());
    assert_eq!(c.f.state().debt(), 660);

    // Instant withdrawal: a ticket created and fulfilled in the same slot.
    // 800 shares are owed 800 × 1061 / 1001 = 847; idle holds 400, and the
    // custodian has sent back the other 447, which the fulfilment pulls in.
    // Whoever paid the ticket's rent has it back once the ticket closes.
    c.send_back(447);
    let rent_payer = c.f.account(&c.f.payer).lamports;
    c.f.ok(&c.f.request_redeem_ix(800, 1));
    assert_eq!(c.f.balance(&c.f.escrow_account), 800);
    assert!(c.f.account(&c.f.payer).lamports < rent_payer);
    c.f.ok(&c.f.fulfil_ix(1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 1_000 + 847);
    assert_eq!(c.f.balance(&c.f.escrow_account), 0);
    assert_eq!(c.f.ticket_shares(1), None);
    assert_eq!(c.f.account(&c.f.payer).lamports, rent_payer);
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (0, 213));
    assert_eq!(c.f.state().total_shares(), 200);

    // The manager brings returned funds to idle at any time.
    c.send_back(100);
    c.f.ok(&c.f.deallocate_ix(100));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (100, 113));

    // The last holder leaves; rounding leaves one unit behind, in the vault's favour.
    c.send_back(112);
    c.f.ok(&c.f.request_redeem_ix(200, 2));
    c.f.ok(&c.f.fulfil_ix(2));
    assert_eq!(
        c.f.balance(&c.f.user_assets),
        USER_BALANCE - 1_000 + 847 + 212
    );
    assert_eq!(c.f.state().total_shares(), 0);
    assert_eq!(c.f.state().total_assets().unwrap(), 1);
}

#[test]
fn a_gated_vault_admits_and_releases_by_permit() {
    let mut c = vault_with(|config| {
        config.deposit_authority = authority_key();
        config.withdraw_authority = authority_key();
        config.fulfil_delay = 3_600;
        config.max_age = 100;
    });
    let later = c.f.now() + 60;

    // A deposit needs the deposit authority's permit: signed off chain, for
    // this depositor, in this vault, and not expired.
    c.f.fails(&c.f.deposit_ix(1_000, 0), VaultError::InvalidPermit);
    let forged = c.f.permit(
        &SigningKey::from_bytes(&[8; 32]),
        PERMIT_DEPOSIT,
        &c.f.user,
        later,
    );
    let someone_elses =
        c.f.permit(&authority(), PERMIT_DEPOSIT, &c.f.stranger, later);
    let wrong_kind = c.f.permit(&authority(), PERMIT_FULFIL, &c.f.user, later);
    for permit in [forged, someone_elses, wrong_kind] {
        c.f.fails(
            &c.f.deposit_with_permit_ix(1_000, 0, &permit),
            VaultError::InvalidPermit,
        );
    }
    let expired =
        c.f.permit(&authority(), PERMIT_DEPOSIT, &c.f.user, c.f.now() - 1);
    c.f.fails(
        &c.f.deposit_with_permit_ix(1_000, 0, &expired),
        VaultError::PermitExpired,
    );
    let permit = c.f.permit(
        &authority(),
        PERMIT_DEPOSIT,
        &c.f.user,
        c.f.now() + 10 * YEAR,
    );
    c.f.ok(&c.f.deposit_with_permit_ix(1_000, 1_000, &permit));

    // A repricing is good for `max_age` slots; then the price is stale again.
    c.f.ok(&c.f.allocate_ix(700));
    c.value(700);
    c.f.advance(100, 0);
    c.f.ok(&c.f.deposit_with_permit_ix(10, 0, &permit));
    c.f.advance(1, 0);
    c.f.fails(
        &c.f.deposit_with_permit_ix(10, 0, &permit),
        VaultError::StaleReport,
    );

    // A loss is taken in full, at once.
    c.value(630);
    assert_eq!(c.f.state().total_assets().unwrap(), 940);

    // While a ticket is younger than the delay, fulfilling it needs the
    // withdrawal authority's permit for that ticket. Anyone holding the
    // permit can present it. Idle covers part; the rest stays open.
    let shares = c.f.balance(&c.f.user_shares);
    c.f.ok(&c.f.request_redeem_ix(shares, 1));
    let fulfil = idle_only(&c.f, c.f.fulfil_ix(1));
    c.f.fails(&fulfil, VaultError::InvalidPermit);
    let permit = c.f.fulfil_permit(&authority(), 1, c.f.now() + 60);
    let fulfil = idle_only(&c.f, c.f.fulfil_with_permit_ix(1, &permit));
    c.f.ok(&fulfil);
    assert_eq!(c.f.state().idle(), 0);
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 1_010 + 310);
    let remaining = c.f.ticket_shares(1).unwrap();
    assert!(0 < remaining && remaining < shares);
    c.f.fails(&fulfil, VaultError::NothingToFulfil);

    // The custodian returns the rest and the manager brings it home; once
    // the delay has passed nobody's permit is needed.
    c.send_back(630);
    c.f.ok(&c.f.deallocate_ix(630));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (630, 0));
    c.f.advance(0, 3_600);
    c.f.ok(&idle_only(&c.f, c.f.fulfil_ix(1)));
    assert_eq!(c.f.ticket_shares(1), None);
    assert_eq!(c.f.state().total_shares(), 0);
    // The one holder leaves with everything the vault still held: 940 of 1_010.
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 1_010 + 940);
    assert_eq!(c.f.state().total_assets().unwrap(), 0);
}

#[test]
fn a_redemption_idle_covers_is_paid_on_the_spot() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));

    // Idle holds 1_000: 400 shares are burned and paid in one instruction.
    // No ticket is created, so nobody pays rent for one.
    let rent_payer = c.f.account(&c.f.payer).lamports;
    c.f.ok(&c.f.request_redeem_ix(400, 1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 600);
    assert_eq!(c.f.balance(&c.f.user_shares), 600);
    assert_eq!(c.f.ticket_shares(1), None);
    assert_eq!(c.f.balance(&c.f.escrow_account), 0);
    assert_eq!(c.f.account(&c.f.payer).lamports, rent_payer);
    assert_eq!((c.f.state().idle(), c.f.state().total_shares()), (600, 600));

    // Idle is short: the same instruction queues the request as a ticket.
    c.f.ok(&c.f.allocate_ix(500));
    c.f.ok(&c.f.request_redeem_ix(300, 2));
    assert_eq!(c.f.ticket_shares(2), Some(300));
    assert_eq!(c.f.balance(&c.f.escrow_account), 300);
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 600);

    // Idle would cover this one, but the price is stale, so it waits too.
    c.f.advance(1, 0);
    c.f.ok(&c.f.request_redeem_ix(50, 3));
    assert_eq!(c.f.ticket_shares(3), Some(50));
}

#[test]
fn cancelling_a_ticket_returns_the_shares() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(800));
    let rent_payer = c.f.account(&c.f.payer).lamports;
    c.f.ok(&c.f.request_redeem_ix(400, 7));
    assert_eq!(c.f.balance(&c.f.user_shares), 600);
    assert_eq!(c.f.ticket_shares(7), Some(400));

    // The shares go back to the owner; naming the escrow itself as where
    // they should go would strand them there.
    let mut into_escrow = c.f.cancel_redeem_ix(7);
    into_escrow.accounts[6].pubkey = c.f.escrow_account;
    c.f.fails(&into_escrow, VaultError::InvalidTokenAccount);

    c.f.ok(&c.f.cancel_redeem_ix(7));
    assert_eq!(c.f.balance(&c.f.user_shares), 1_000);
    assert_eq!(c.f.ticket_shares(7), None);
    assert_eq!(c.f.state().total_shares(), 1_000);
    assert_eq!(c.f.account(&c.f.payer).lamports, rent_payer);
}

#[test]
fn views_answer_through_return_data() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(500));
    c.f.advance(1, YEAR);
    c.value(1_000); // total assets 1_500 over 1_000 shares

    assert_eq!(
        c.f.view(VIEW_CONVERT_TO_SHARES, &300u64.to_le_bytes()),
        300 * 1_001 / 1_501
    );
    assert_eq!(
        c.f.view(VIEW_CONVERT_TO_ASSETS, &300u64.to_le_bytes()),
        300 * 1_501 / 1_001
    );
    assert_eq!(c.f.view(VIEW_MAX_DEPOSIT, &[]), u64::MAX - 1_500);
}

// ---- Repricing and fees ----

#[test]
fn a_gain_unlocks_over_the_period_and_a_loss_lands_at_once() {
    let mut c = vault_with(|config| {
        config.unlock_period = YEAR as u64;
        config.performance_fee_bps = 2_000; // 20% of gains
    });
    c.f.ok(&c.f.deposit_ix(10_000, 0));
    c.f.ok(&c.f.allocate_ix(10_000));
    let price = |c: &mut Custodied| c.f.view(VIEW_CONVERT_TO_ASSETS, &10_000u64.to_le_bytes());

    // The oracle reports 2_000 of gain. It is counted and locked: the share
    // price has not moved, and no fee has been earned yet.
    c.value(12_000);
    assert_eq!((c.f.state().debt(), c.f.state().locked()), (12_000, 2_000));
    assert_eq!(price(&mut c), 10_000);
    assert_eq!(c.f.state().fee_shares(), 0);

    // Half the period later half of it has unlocked. The fee on that 1_000
    // is 200 of assets, paid as shares worth 200: 200 × 10_001 / (11_001 − 200) = 185.
    c.f.advance(1, YEAR / 2);
    c.f.reprice();
    assert_eq!(c.f.state().locked(), 1_000);
    assert_eq!(c.f.state().total_assets().unwrap(), 11_000);
    assert_eq!(c.f.state().fee_shares(), 185);
    assert_eq!(c.f.state().total_shares(), 10_185);

    // A loss lands in full in the same repricing. What was still locked was
    // never in the price, so it goes first; nothing is earned, so no fee.
    let fee_shares = c.f.state().fee_shares();
    c.value(5_000);
    assert_eq!((c.f.state().debt(), c.f.state().locked()), (5_000, 0));
    assert_eq!(c.f.state().total_assets().unwrap(), 5_000);
    assert_eq!(c.f.state().fee_shares(), fee_shares);

    // Fee shares are minted on demand, once, and then every share exists.
    c.f.ok(&c.f.collect_fees_ix());
    assert_eq!(c.f.balance(&c.f.fee_shares), fee_shares);
    assert_eq!(c.f.state().fee_shares(), 0);
    let supply = u64::from_le_bytes(
        c.f.account(&c.f.share_mint).data[36..44]
            .try_into()
            .unwrap(),
    );
    assert_eq!(supply, c.f.state().total_shares());
    c.f.fails(&c.f.collect_fees_ix(), VaultError::ZeroAmount);
}

#[test]
fn a_second_gain_joins_the_first_without_restarting_it() {
    let mut c = vault_with(|config| config.unlock_period = 100);
    c.f.ok(&c.f.deposit_ix(10_000, 0));
    c.f.ok(&c.f.allocate_ix(10_000));

    // 1_000 locks for 100 seconds. Sixty seconds in, 600 has unlocked and
    // 400 has 40 seconds left; another 400 arrives with a full 100. Together
    // they unlock over the average of what each had left: 70 seconds.
    c.value(11_000);
    c.f.advance(1, 60);
    c.value(11_400);
    assert_eq!(c.f.state().locked(), 800);
    c.f.advance(1, 35);
    c.f.reprice();
    assert_eq!(c.f.state().locked(), 400);
    c.f.advance(1, 35);
    c.f.reprice();
    assert_eq!(c.f.state().locked(), 0);
    assert_eq!(c.f.state().total_assets().unwrap(), 11_400);
}

#[test]
fn management_fee_accrues_with_time() {
    let mut c = vault_with(|config| config.management_fee_bps = 100); // 1% a year
    c.f.ok(&c.f.deposit_ix(10_000, 0));
    c.f.ok(&c.f.allocate_ix(10_000));

    // 1% of 10_000 for one year is 100 of assets: 100 × 10_001 / 9_901 = 101 shares.
    c.f.advance(1, YEAR);
    c.value(10_000);
    assert_eq!(c.f.state().fee_shares(), 101);
}

#[test]
fn new_money_pays_no_management_fee_for_time_before_it() {
    let mut c = vault_with(|config| config.management_fee_bps = 500); // 5% a year

    // A year passes over an empty vault, then over ten units of dust: too
    // little for any fee to round up, so the fee clock never moved.
    c.f.advance(1, YEAR);
    c.f.ok(&c.f.deposit_ix(10, 0));
    c.f.ok(&c.f.allocate_ix(10));
    c.f.advance(1, YEAR);
    c.value(10);
    assert_eq!(c.f.state().fee_shares(), 0);

    // A large deposit lands and the vault is repriced in the same second.
    // It owes nothing for the two years it was not there.
    c.f.ok(&c.f.deposit_ix(500_000, 0));
    c.f.reprice();
    assert_eq!(c.f.state().fee_shares(), 0);

    // From here it pays for the time it holds: 5% of 500_010 over a year is
    // 25_000, paid in shares that round down to one unit less.
    c.f.advance(1, YEAR);
    c.f.reprice();
    let fee_shares = c.f.state().fee_shares();
    assert_eq!(c.f.state().to_assets(fee_shares).unwrap(), 24_999);
}

#[test]
fn recovering_a_loss_earns_no_performance_fee() {
    let mut c = vault_with(|config| config.performance_fee_bps = 5_000);
    c.f.ok(&c.f.deposit_ix(10_000, 0));
    c.f.ok(&c.f.allocate_ix(10_000));

    // Down to 5_000 and back: holders are where they started, so no fee.
    c.value(5_000);
    c.value(10_000);
    assert_eq!(c.f.state().debt(), 10_000);
    assert_eq!(c.f.state().fee_shares(), 0);

    // Only what rises above the old level is earned: half of 2_000, paid in
    // shares that round down to one unit less.
    c.value(12_000);
    let fee_shares = c.f.state().fee_shares();
    assert_eq!(c.f.state().to_assets(fee_shares).unwrap(), 999);
}

#[test]
fn a_written_off_loss_is_a_loss_like_any_other() {
    let mut c = vault_with(|config| config.performance_fee_bps = 5_000);
    c.f.ok(&c.f.deposit_ix(10_000, 0));
    c.f.ok(&c.f.allocate_ix(10_000));

    // The guardian recognises that 4_000 is gone. The price falls at once.
    c.f.fails(
        &c.f.write_off_ix(c.f.stranger, 0),
        VaultError::InvalidAuthority,
    );
    c.f.fails(
        &c.f.write_off_ix(c.f.guardian, 10_001),
        VaultError::AmountTooLarge,
    );
    c.f.ok(&c.f.write_off_ix(c.f.guardian, 6_000));
    assert_eq!(c.f.state().total_assets().unwrap(), 6_000);
    assert_eq!(
        c.f.view(VIEW_CONVERT_TO_ASSETS, &10_000u64.to_le_bytes()),
        6_000
    );

    // If it comes back after all, making it up earns no performance fee.
    c.value(10_000);
    assert_eq!(c.f.state().debt(), 10_000);
    assert_eq!(c.f.state().fee_shares(), 0);
}

#[test]
fn what_comes_back_after_a_full_write_off_goes_to_the_holders() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));

    // Everything is written off. The shares are worth nothing, and a vault
    // in that state takes no deposit: the first unit in would otherwise buy
    // nearly all of whatever comes back.
    c.f.ok(&c.f.write_off_ix(c.f.guardian, 0));
    c.f.fails(&c.f.deposit_ix(10, 0), VaultError::Worthless);

    // 600 is recovered after all. The oracle marks the book at it, the
    // custodian sends it back, and the manager brings it home.
    c.value(600);
    c.send_back(600);
    c.f.ok(&c.f.deallocate_ix(600));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (600, 0));

    // It belongs to the holders who took the loss.
    c.f.ok(&c.f.request_redeem_ix(1_000, 1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE - 1_000 + 600);
}

#[test]
fn tokens_sent_to_the_vault_unlock_like_any_gain() {
    let mut c = vault_with(|config| config.unlock_period = YEAR as u64);
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    let price = |c: &mut Custodied| c.f.view(VIEW_CONVERT_TO_ASSETS, &1_000u64.to_le_bytes());

    // Someone sends 5_000 straight to the idle account, as an incentive. On
    // its own that changes nothing; a repricing counts it, and locks it.
    c.f.set_balance(c.f.idle_account, 6_000);
    assert_eq!(price(&mut c), 1_000);
    c.f.reprice();
    assert_eq!((c.f.state().idle(), c.f.state().locked()), (6_000, 5_000));
    assert_eq!(price(&mut c), 1_000);

    // It reaches the holders evenly over the period.
    c.f.advance(1, YEAR / 4);
    c.f.reprice();
    assert_eq!(c.f.state().total_assets().unwrap(), 2_250);
    c.f.advance(1, YEAR);
    c.f.reprice();
    assert_eq!(c.f.state().total_assets().unwrap(), 6_000);
    assert_eq!(c.f.state().idle(), c.f.balance(&c.f.idle_account));
}

#[test]
fn entering_while_gains_are_locked_takes_none_of_them() {
    let mut c = vault_with(|config| config.unlock_period = YEAR as u64);
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.request_redeem_ix(500, 9)); // the holder keeps 500 shares
    c.f.set_balance(c.f.idle_account, 5_500);
    c.f.reprice();
    assert_eq!(
        (c.f.state().total_assets().unwrap(), c.f.state().locked()),
        (500, 5_000)
    );

    // A deposit is priced against everything the vault has counted, locked
    // gains included: 99_000 buys shares at 11 each, not at 1. Leaving at
    // once redeems at 1 plus its own money, so it comes back short. Capital
    // that arrives for a gain and leaves right after pays for the visit.
    let before = c.f.balance(&c.f.user_assets);
    c.f.ok(&c.f.deposit_ix(99_000, 0));
    let bought = c.f.balance(&c.f.user_shares) - 500;
    assert_eq!(bought, 99_000 * 501 / 5_501);
    c.f.ok(&c.f.request_redeem_ix(bought, 1));
    assert!(c.f.balance(&c.f.user_assets) < before);
    assert_eq!(c.f.state().locked(), 5_000);
}

#[test]
fn a_recovery_cannot_be_bought_cheaply_while_it_unlocks() {
    let mut c = vault_with(|config| config.unlock_period = 7 * 24 * 3_600);
    c.f.ok(&c.f.deposit_ix(900_000, 0));
    c.f.ok(&c.f.allocate_ix(900_000));

    // Everything is written off, then found again: the recovery is counted
    // and locked, so the shares still redeem for nothing.
    c.f.ok(&c.f.write_off_ix(c.f.guardian, 0));
    c.value(900_000);
    assert_eq!(c.f.state().total_assets().unwrap(), 0);

    // A second later one unit has unlocked. Someone deposits ten units,
    // hoping to buy most of the vault for it. A deposit is priced against
    // everything counted, so ten units buy ten shares.
    c.f.advance(1, 1);
    c.f.reprice();
    c.f.ok(&c.f.deposit_ix(10, 0));
    assert_eq!(c.f.balance(&c.f.user_shares), 900_000 + 10);
}

#[test]
fn tokens_taken_from_the_vault_are_a_loss_at_once() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));

    // An issuer claws back 400 from the idle account, as a permanent
    // delegate can. The next repricing counts what is really there.
    c.f.set_balance(c.f.idle_account, 600);
    c.f.reprice();
    assert_eq!(c.f.state().idle(), 600);
    assert_eq!(
        c.f.view(VIEW_CONVERT_TO_ASSETS, &1_000u64.to_le_bytes()),
        600
    );
}

// ---- Limits and exits ----

#[test]
fn deposits_and_allocations_respect_their_limits() {
    let mut c = vault_with(|config| {
        config.debt_cap = 300;
        config.deposit_cap = 1_500;
    });
    c.f.fails(&c.f.deposit_ix(0, 0), VaultError::ZeroAmount);
    c.f.fails(&c.f.deposit_ix(1_000, 1_001), VaultError::SlippageExceeded);
    c.f.ok(&c.f.deposit_ix(1_000, 1_000));

    c.f.fails(&c.f.deposit_ix(501, 0), VaultError::AmountTooLarge);
    c.f.ok(&c.f.deposit_ix(500, 0));
    c.f.fails(&c.f.allocate_ix(301), VaultError::AmountTooLarge);
    c.f.ok(&c.f.allocate_ix(300));
    c.f.fails(&c.f.allocate_ix(5_000), VaultError::AmountTooLarge);

    let mut unsigned = c.f.allocate_ix(1);
    unsigned.accounts[0].is_signer = false;
    c.f.fails(&unsigned, VaultError::NotSigner);
    let mut stranger = c.f.allocate_ix(1);
    stranger.accounts[0].pubkey = c.f.stranger;
    c.f.fails(&stranger, VaultError::InvalidAuthority);
}

#[test]
fn nothing_blocks_an_exit() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(900));
    c.f.ok(&c.f.request_redeem_ix(300, 1));
    assert_eq!(c.f.ticket_shares(1), Some(300));

    // A pause stops new money and nothing else.
    c.f.fails(
        &c.f.set_paused_ix(c.f.stranger, true),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.set_paused_ix(c.f.guardian, true));
    c.f.fails(&c.f.deposit_ix(100, 0), VaultError::InvalidStatus);
    c.f.fails(&c.f.allocate_ix(100), VaultError::InvalidStatus);
    c.f.ok(&c.price_ix(&oracle(), PRICE_SCALE, 3_600));
    c.send_back(200);
    c.f.ok(&c.f.fulfil_ix(1));
    assert_eq!(c.f.ticket_shares(1), None);
    c.f.ok(&c.f.request_redeem_ix(100, 2));
    c.f.ok(&c.f.cancel_redeem_ix(2));
    c.f.ok(&c.f.set_paused_ix(c.f.guardian, false));
    c.f.ok(&c.f.deposit_ix(100, 0));

    // Wind-down is the owner's, and permanent. Funds still come home, and a
    // redemption idle covers is still paid on the spot.
    c.f.fails(
        &c.f.wind_down_ix(c.f.guardian),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.wind_down_ix(c.f.owner));
    c.f.fails(
        &c.f.set_paused_ix(c.f.guardian, false),
        VaultError::InvalidStatus,
    );
    c.f.fails(&c.f.deposit_ix(100, 0), VaultError::InvalidStatus);
    c.f.fails(&c.f.allocate_ix(100), VaultError::InvalidStatus);
    c.send_back(100);
    c.f.ok(&c.f.deallocate_ix(100));
    let before = c.f.balance(&c.f.user_assets);
    c.f.ok(&c.f.request_redeem_ix(100, 3));
    assert_eq!(c.f.balance(&c.f.user_assets), before + 100);
    assert_eq!(c.f.ticket_shares(3), None);

    // The oracle goes silent: its last price expires, the adapter stops
    // answering, and nothing can be priced, so a request waits as a ticket.
    // The guardian writes the strategy off; with no debt left there is
    // nothing to reprice, and holders exit against idle without a report.
    c.f.advance(1_000, 3_601);
    let reprice = c.f.simulate_ix();
    assert!(c.f.run(&reprice).program_result.is_err());
    c.f.ok(&c.f.request_redeem_ix(100, 4));
    let fulfil = idle_only(&c.f, c.f.fulfil_ix(4));
    c.f.fails(&fulfil, VaultError::StaleReport);
    c.f.ok(&c.f.write_off_ix(c.f.guardian, 0));
    assert_eq!(c.f.state().debt(), 0);
    let before = c.f.balance(&c.f.user_assets);
    c.f.ok(&fulfil);
    assert!(c.f.balance(&c.f.user_assets) > before);
}

#[test]
fn shares_are_never_burned_for_nothing() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));
    c.f.ok(&c.f.request_redeem_ix(500, 1));

    // The strategy is marked to zero for a moment. A ticket is worth nothing
    // right now, and nobody can use that to wipe it out.
    c.value(0);
    c.f.fails(&c.f.fulfil_ix(1), VaultError::NothingToFulfil);
    assert_eq!(c.f.ticket_shares(1), Some(500));
}

// ---- Who can change what, and when ----

#[test]
fn configuration_changes_wait_out_the_timelock() {
    let mut c = Custodied::new();
    let mut config = c.f.config;
    config.max_age = 5;
    config.performance_fee_bps = 1_000;

    c.f.fails(
        &c.f.execute_config_ix(c.f.owner),
        VaultError::TimelockNotPassed,
    );
    c.f.fails(
        &c.f.submit_config_ix(c.f.stranger, &config),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.submit_config_ix(c.f.owner, &config));

    // Submitted is not applied.
    assert_eq!(c.f.state().config.max_age(), 0);
    c.f.advance(1, 999);
    c.f.fails(
        &c.f.execute_config_ix(c.f.owner),
        VaultError::TimelockNotPassed,
    );
    c.f.advance(1, 1);
    c.f.fails(
        &c.f.execute_config_ix(c.f.stranger),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.execute_config_ix(c.f.owner));
    assert_eq!(c.f.state().config.max_age(), 5);
    assert_eq!(c.f.state().config.performance_fee_bps(), 1_000);
    c.f.fails(
        &c.f.execute_config_ix(c.f.owner),
        VaultError::TimelockNotPassed,
    );

    // Fees and the fulfil delay have ceilings no owner can exceed.
    for exceed in [
        |c: &mut ConfigArgs| c.performance_fee_bps = 5_001,
        |c: &mut ConfigArgs| c.management_fee_bps = 501,
        |c: &mut ConfigArgs| c.fulfil_delay = 90 * 24 * 60 * 60 + 1,
    ] {
        let mut config = c.f.config;
        exceed(&mut config);
        c.f.fails(
            &c.f.submit_config_ix(c.f.owner, &config),
            VaultError::InvalidConfig,
        );
    }
}

#[test]
fn ownership_moves_in_two_steps() {
    let mut c = Custodied::new();
    let next = c.f.stranger;
    c.f.fails(
        &c.f.transfer_ownership_ix(next, next),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.transfer_ownership_ix(c.f.owner, next));

    // Named is not owner yet.
    c.f.fails(&c.f.wind_down_ix(next), VaultError::InvalidAuthority);
    c.f.fails(
        &c.f.accept_ownership_ix(c.f.guardian),
        VaultError::InvalidAuthority,
    );
    c.f.ok(&c.f.accept_ownership_ix(next));
    c.f.fails(&c.f.wind_down_ix(c.f.owner), VaultError::InvalidAuthority);
    c.f.ok(&c.f.wind_down_ix(next));
}

// ---- Which accounts and tokens a vault accepts ----

/// Flip one byte of one account before creation and expect `error`.
fn rejects(account: fn(&Fixture) -> Pubkey, offset: usize, error: VaultError) {
    let mut c = Custodied::setup(TOKEN);
    let key = account(&c.f);
    c.f.account_mut(&key).data[offset] ^= 1;
    c.f.fails(&c.f.create_vault_ix(), error);
}

#[test]
fn a_vault_only_accepts_accounts_it_alone_controls() {
    // Share mint: the vault must be its mint authority. The rest is its
    // creator's business.
    rejects(|f| f.share_mint, 4, VaultError::InvalidMint);

    // Idle account: another owner, a delegate, or a close authority.
    rejects(|f| f.idle_account, 32, VaultError::InvalidTokenAccount);
    rejects(|f| f.idle_account, 72, VaultError::InvalidTokenAccount);
    rejects(|f| f.idle_account, 129, VaultError::InvalidTokenAccount);

    // Escrow and strategy accounts: the same rules.
    rejects(|f| f.escrow_account, 129, VaultError::InvalidTokenAccount);
    rejects(|f| f.strategy_account, 32, VaultError::InvalidTokenAccount);

    // None of the vault's token accounts may demand a memo on what it
    // receives: the program's own transfers carry none, so that account
    // would refuse them.
    let mut c = Custodied::setup_with_shares(TOKEN, TOKEN_2022);
    let escrow = c.f.escrow_account;
    let data = &mut c.f.account_mut(&escrow).data;
    data.push(2); // account type: token account
    data.extend(8u16.to_le_bytes()); // MemoTransfer
    data.extend(1u16.to_le_bytes());
    data.push(1); // required
    c.f.fails(&c.f.create_vault_ix(), VaultError::InvalidTokenAccount);

    // The asset and the shares are two different mints.
    let mut c = Custodied::setup(TOKEN);
    let mut ix = c.f.create_vault_ix();
    ix.accounts[3].pubkey = ix.accounts[4].pubkey;
    c.f.fails(&ix, VaultError::InvalidMint);

    // A vault always has an adapter.
    let mut c = Custodied::setup(TOKEN);
    c.f.adapter = SYSTEM;
    c.f.fails(&c.f.create_vault_ix(), VaultError::InvalidAdapter);

    // The vault address is derived from the share mint, and created once.
    let mut c = Custodied::new();
    c.f.fails(&c.f.create_vault_ix(), VaultError::AlreadyInitialized);
}

#[test]
fn token_2022_assets_work_unless_a_transfer_hook_is_set() {
    // A plain Token-2022 asset works end to end, adapter included.
    let mut c = Custodied::setup(TOKEN_2022);
    c.open();
    c.f.ok(&c.f.deposit_ix(1_000, 1_000));
    c.f.ok(&c.f.allocate_ix(400));
    c.value(400);
    c.send_back(400);
    c.f.ok(&c.f.request_redeem_ix(1_000, 1));
    c.f.ok(&c.f.fulfil_ix(1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE);

    let with_extensions = |extensions: &[(u16, Vec<u8>)]| {
        let mut c = Custodied::setup(TOKEN_2022);
        let mint = c.f.asset_mint;
        *c.f.account_mut(&mint) = mint_account(&TOKEN_2022, None, extensions);
        c.f
    };
    const TRANSFER_HOOK: u16 = 14;
    let hook = |program: u8| (TRANSFER_HOOK, [[0; 32], [program; 32]].concat());

    // Every other extension is the manager's call: transfer fee, mint close
    // authority, default account state, permanent delegate, pausable.
    for extension in [1, 3, 6, 12, 26] {
        with_extensions(&[(extension, vec![0; 8])]).create();
    }

    // A hook extension with no program set runs nothing, so it passes.
    with_extensions(&[hook(0)]).create();

    // A hook program is rejected, wherever it sits in the list.
    let mut f = with_extensions(&[hook(7)]);
    f.fails(&f.create_vault_ix(), VaultError::InvalidMint);
    let mut f = with_extensions(&[(12, vec![0; 32]), hook(7)]);
    f.fails(&f.create_vault_ix(), VaultError::InvalidMint);

    // So is a list that does not parse.
    let mut f = with_extensions(&[(12, vec![0; 32])]);
    let mint = f.asset_mint;
    f.account_mut(&mint).data.truncate(180);
    f.fails(&f.create_vault_ix(), VaultError::InvalidMint);
}

#[test]
fn shares_can_be_a_token_2022_mint_of_the_creators_choosing() {
    const TRANSFER_HOOK: u16 = 14;
    const PERMANENT_DELEGATE: u16 = 12;
    let with_share_mint = |extensions: &[(u16, Vec<u8>)], freeze: bool| {
        let mut c = Custodied::setup_with_shares(TOKEN, TOKEN_2022);
        let (mint, vault) = (c.f.share_mint, c.f.vault);
        let mut account = mint_account(&TOKEN_2022, Some(&vault), extensions);
        account.data[46] = freeze as u8;
        *c.f.account_mut(&mint) = account;
        c
    };

    // Token-2022 shares with a freeze authority and a permanent delegate:
    // the creator's choice. The vault works end to end on them.
    let mut c = with_share_mint(&[(PERMANENT_DELEGATE, vec![9; 32])], true);
    c.open();
    c.f.ok(&c.f.deposit_ix(1_000, 1_000));
    // Paid on the spot: shares burned straight from the holder.
    c.f.ok(&c.f.request_redeem_ix(400, 1));
    assert_eq!(c.f.balance(&c.f.user_shares), 600);
    // Queued, cancelled, queued again and fulfilled: shares through escrow.
    c.f.ok(&c.f.allocate_ix(600));
    c.f.ok(&c.f.request_redeem_ix(600, 2));
    c.f.ok(&c.f.cancel_redeem_ix(2));
    c.f.ok(&c.f.request_redeem_ix(600, 3));
    c.value(600);
    c.send_back(600);
    c.f.ok(&c.f.fulfil_ix(3));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE);
    assert_eq!(c.f.state().total_shares(), 0);

    // The one thing a share mint cannot have is a transfer hook program.
    let hook = (TRANSFER_HOOK, [[0; 32], [7; 32]].concat());
    let mut c = with_share_mint(&[hook], false);
    c.f.fails(&c.f.create_vault_ix(), VaultError::InvalidMint);
}

/// Turn the vault's asset into a Token-2022 mint that withholds 1% of every
/// transfer. The config is two authorities, the withheld total, then the
/// older and newer fee as (epoch, maximum fee, basis points); each token
/// account of that mint records the fees withheld in it.
fn with_transfer_fee(c: &mut Custodied) {
    const TRANSFER_FEE_CONFIG: u16 = 1;
    const TRANSFER_FEE_AMOUNT: u16 = 2;
    let fee = [
        &0u64.to_le_bytes()[..],
        &u64::MAX.to_le_bytes(),
        &100u16.to_le_bytes(),
    ]
    .concat();
    let config = [&[0; 72][..], &fee, &fee].concat();
    let mint = c.f.asset_mint;
    *c.f.account_mut(&mint) = mint_account(&TOKEN_2022, None, &[(TRANSFER_FEE_CONFIG, config)]);
    for key in [
        c.f.idle_account,
        c.f.user_assets,
        c.f.strategy_account,
        c.destination,
        c.return_account,
    ] {
        let data = &mut c.f.account_mut(&key).data;
        data.push(2); // account type: token account
        data.extend(TRANSFER_FEE_AMOUNT.to_le_bytes());
        data.extend(8u16.to_le_bytes());
        data.extend([0; 8]);
    }
}

#[test]
fn a_transfer_fee_is_counted_never_assumed() {
    let mut c = Custodied::setup(TOKEN_2022);
    with_transfer_fee(&mut c);
    c.open();

    // 1_000 sent, 990 arrive: the depositor gets shares for 990.
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    assert_eq!(c.f.state().idle(), 990);
    assert_eq!(c.f.balance(&c.f.user_shares), 990);

    // 500 leave idle, 495 reach the strategy account: that is the debt. The
    // adapter's own transfer to the custodian loses another 1%.
    c.f.ok(&c.f.allocate_ix(500));
    assert_eq!((c.f.state().idle(), c.f.state().debt()), (490, 495));
    assert_eq!(c.f.balance(&c.destination), 490);

    // Coming back costs the fee twice more; each hop is counted as it lands.
    c.value(490);
    c.send_back(490);
    c.f.ok(&c.f.deallocate_ix(490));
    assert!(c.f.state().idle() < 490 + 490);

    // Whatever happened, the vault counts exactly what it holds.
    assert_eq!(c.f.state().idle(), c.f.balance(&c.f.idle_account));
}

#[test]
fn a_new_configuration_settles_the_old_one_first() {
    let mut c = vault_with(|config| config.management_fee_bps = 500); // 5% a year
    c.f.ok(&c.f.deposit_ix(1_000_000, 0));

    // A year under the 5% fee, then a new configuration takes over. The fee
    // the old one earned is charged as it is replaced, not forgiven.
    let config = c.f.config;
    c.f.ok(&c.f.submit_config_ix(c.f.owner, &config));
    c.f.advance(1, YEAR);
    c.f.ok(&c.f.execute_config_ix(c.f.owner));
    let fee_shares = c.f.state().fee_shares();
    assert_eq!(c.f.state().to_assets(fee_shares).unwrap(), 49_999);
}

#[test]
fn a_fulfil_permit_is_good_for_one_ticket_only() {
    let mut c = vault_with(|config| {
        config.withdraw_authority = authority_key();
        config.fulfil_delay = 3_600;
    });
    c.f.ok(&c.f.deposit_ix(1_000, 0));

    // The authority permits a ticket of 10 shares, and it is fulfilled.
    c.f.ok(&c.f.request_redeem_ix(10, 1));
    let permit = c.f.fulfil_permit(&authority(), 1, c.f.now() + 600);
    c.f.ok(&c.f.fulfil_with_permit_ix(1, &permit));
    assert_eq!(c.f.ticket_shares(1), None);

    // The owner opens a much larger ticket under the same id, so at the same
    // address. The old permit, still unexpired, named another ticket.
    c.f.ok(&c.f.request_redeem_ix(900, 1));
    c.f.fails(
        &c.f.fulfil_with_permit_ix(1, &permit),
        VaultError::InvalidPermit,
    );
    let permit = c.f.fulfil_permit(&authority(), 1, c.f.now() + 600);
    c.f.ok(&c.f.fulfil_with_permit_ix(1, &permit));
}

#[test]
fn a_transfer_fee_on_the_way_home_is_a_loss_not_a_fault() {
    let mut c = Custodied::setup(TOKEN_2022);
    with_transfer_fee(&mut c);
    c.f.config.unlock_period = 7 * 24 * 3_600;
    c.open();
    c.f.ok(&c.f.deposit_ix(100_000, 0));
    c.f.ok(&c.f.allocate_ix(99_000));

    // Written off, found again and locked, then brought home through two
    // transfers that each withhold 1%. What the fee took comes out of the
    // locked gain, so the vault's count never exceeds what it holds and it
    // keeps pricing.
    c.f.ok(&c.f.write_off_ix(c.f.guardian, 0));
    c.value(90_000);
    c.send_back(90_000);
    c.f.ok(&c.f.deallocate_ix(90_000));
    assert!(c.f.state().locked() <= c.f.state().counted_assets().unwrap());
    c.f.reprice();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    assert_eq!(c.f.state().idle(), c.f.balance(&c.f.idle_account));
}

#[test]
fn an_unlock_period_beyond_the_clock_locks_gains_for_good_and_nothing_breaks() {
    let mut c = vault_with(|config| config.unlock_period = u64::MAX);
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(1_000));

    // Two gains in a row, the second joining the first: both are counted,
    // neither unlocks, and the vault keeps repricing and paying.
    c.value(1_500);
    c.f.advance(1, YEAR);
    c.value(2_000);
    assert_eq!(c.f.state().locked(), 1_000);
    assert_eq!(c.f.state().total_assets().unwrap(), 1_000);
    c.send_back(1_000);
    c.f.ok(&c.f.request_redeem_ix(1_000, 1));
    c.f.ok(&c.f.fulfil_ix(1));
    assert_eq!(c.f.balance(&c.f.user_assets), USER_BALANCE);
}
