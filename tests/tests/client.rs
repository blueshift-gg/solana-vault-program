//! The generated Rust client against the program. `idl/vault.ts` is a second,
//! hand-written encoding of the wire format; these tests are what keeps it
//! from drifting. Every generated instruction must equal the fixture's
//! hand-built one and be accepted by the program, and every account must
//! decode to what `vault_core::state` reads from the same bytes.

use custody_adapter::constants::PRICE_SCALE;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;
use vault_core::constants::{
    PERMIT_DEPOSIT, PERMIT_DOMAIN, PERMIT_FULFIL, STRATEGY_SEED, TICKET_LEN, VAULT_LEN,
};
use vault_core::errors::VaultError;
use vault_core::state;
use vault_rust::accounts::{Ticket, Vault};
use vault_rust::errors::SolanaVaultError;
use vault_rust::instructions::*;
use vault_rust::types::{Config, Permit, PermitKind, PermitMessage};
use vault_tests::custody::*;
use vault_tests::*;

/// The generated instruction, once it equals the hand-built one.
fn same(generated: Instruction, by_hand: Instruction) -> Instruction {
    assert_eq!(generated, by_hand);
    generated
}

fn config(args: &ConfigArgs) -> Config {
    Config {
        manager: args.manager,
        guardian: args.guardian,
        fee_recipient: args.fee_recipient,
        deposit_authority: args.deposit_authority,
        withdraw_authority: args.withdraw_authority,
        debt_cap: args.debt_cap,
        deposit_cap: args.deposit_cap,
        max_age: args.max_age,
        fulfil_delay: args.fulfil_delay,
        unlock_period: args.unlock_period,
        performance_fee_bps: args.performance_fee_bps,
        management_fee_bps: args.management_fee_bps,
    }
}

/// Assert that each listed field of a decoded account equals the getter of
/// the same name on the `vault_core` view. Keys compare as bytes.
macro_rules! assert_fields {
    ($decoded:expr, $state:expr, keys: [$($key:ident),*], numbers: [$($number:ident),*]) => {
        $(assert_eq!(&$decoded.$key.to_bytes(), $state.$key(), stringify!($key));)*
        $(assert_eq!($decoded.$number, $state.$number(), stringify!($number));)*
    };
}

fn assert_config(decoded: &Config, state: &state::Config) {
    assert_fields!(
        decoded, state,
        keys: [manager, guardian, fee_recipient, deposit_authority, withdraw_authority],
        numbers: [
            debt_cap, deposit_cap, max_age, fulfil_delay,
            unlock_period, performance_fee_bps, management_fee_bps
        ]
    );
}

fn assert_vault(f: &Fixture) {
    let data = &f.account(&f.vault).data;
    let (decoded, state) = (Vault::from_bytes(data).unwrap(), f.state());
    assert_eq!(Vault::LEN, VAULT_LEN);
    // The fields cover every byte, in order.
    assert_eq!(&borsh::to_vec(&decoded).unwrap(), data);
    assert_eq!(decoded.status as u8, state.status());
    assert_fields!(
        decoded, state,
        keys: [
            owner, pending_owner, asset_mint, asset_token_program, share_mint, idle_account,
            escrow_account, adapter, strategy_authority, strategy_account
        ],
        numbers: [
            version, bump, strategy_bump, decimals, idle, debt, total_shares, fee_shares,
            last_report_slot, unlock_ts, fee_ts, loss, timelock, pending_at, locked, unlock_end,
            tickets
        ]
    );
    assert_config(&decoded.config, &state.config);
    assert_config(&decoded.pending, &state.pending);
}

fn assert_ticket(f: &Fixture, id: u64) {
    let data = &f.account(&f.ticket(id)).data;
    assert_eq!((data.len(), Ticket::LEN), (TICKET_LEN, TICKET_LEN));
    let decoded = Ticket::from_bytes(data).unwrap();
    // SAFETY: the length matches the layout, which has alignment 1.
    let state = unsafe { state::Ticket::from_bytes_unchecked(data) };
    assert_eq!(&borsh::to_vec(&decoded).unwrap(), data);
    assert_fields!(
        decoded, state,
        keys: [vault, owner, payer],
        numbers: [version, bump, shares, created_at, nonce]
    );
}

/// The fixture's permit as the generated type, once the generated message
/// type has been held to the bytes the authority signed.
fn permit(f: &Fixture, kind: PermitKind, subject: &Pubkey, nonce: u64) -> Permit {
    let expires_at = f.now() + YEAR;
    let message = PermitMessage {
        domain: *PERMIT_DOMAIN,
        program: PROGRAM_ID,
        kind,
        vault: f.vault,
        subject: *subject,
        nonce,
        expires_at,
    };
    let signed = vault_core::permit_message(
        kind as u8,
        &f.vault.to_bytes(),
        &subject.to_bytes(),
        nonce,
        expires_at,
    );
    assert_eq!(borsh::to_vec(&message).unwrap(), signed);
    let bytes = f.permit_with_nonce(&authority(), kind as u8, subject, nonce, expires_at);
    let permit = Permit {
        expires_at,
        signature: bytes[8..].try_into().unwrap(),
    };
    assert_eq!(borsh::to_vec(&permit).unwrap(), bytes);
    permit
}

fn request_redeem(f: &Fixture, shares: u64, id: u64) -> Instruction {
    let (ticket, bump) = Ticket::find_pda(&f.vault, &f.user, id);
    let generated = RequestRedeemBuilder::new()
        .payer(f.payer)
        .owner(f.user)
        .vault(f.vault)
        .ticket(ticket)
        .share_mint(f.share_mint)
        .owner_shares(f.user_shares)
        .escrow_account(f.escrow_account)
        .idle_account(f.idle_account)
        .asset_mint(f.asset_mint)
        .destination(f.user_assets)
        .share_token_program(f.share_program)
        .asset_token_program(f.token_program)
        .shares(shares)
        .id(id)
        .bump(bump)
        .instruction();
    same(generated, f.request_redeem_ix(shares, id))
}

/// One vault through every instruction. A performance fee makes `CollectFees`
/// mint; a deposit and a withdrawal authority make both permits necessary.
#[test]
fn generated_instructions_run_and_accounts_decode() {
    let mut c = Custodied::setup(TOKEN);
    let f = &mut c.f;
    f.config.performance_fee_bps = 1_000;
    f.config.deposit_authority = authority_key();
    f.config.withdraw_authority = authority_key();
    f.config.fulfil_delay = 3_600;
    f.config.unlock_period = 86_400;
    assert_eq!(
        (PermitKind::Deposit as u8, PermitKind::Fulfil as u8),
        (PERMIT_DEPOSIT, PERMIT_FULFIL)
    );
    let own = f.adapter_own.clone();
    let prefix = f.adapter_accounts("");
    let adapter = |call| [&prefix[..], &own[call]].concat();

    let (vault, bump) = Vault::find_pda(&f.share_mint);
    assert_eq!(vault, f.vault);
    let strategy_bump =
        Pubkey::find_program_address(&[STRATEGY_SEED, vault.as_ref()], &PROGRAM_ID).1;
    let generated = CreateVaultBuilder::new()
        .payer(f.payer)
        .owner(f.owner)
        .vault(vault)
        .asset_mint(f.asset_mint)
        .share_mint(f.share_mint)
        .idle_account(f.idle_account)
        .escrow_account(f.escrow_account)
        .strategy_account(f.strategy_account)
        .bump(bump)
        .strategy_bump(strategy_bump)
        .adapter(f.adapter)
        .timelock(f.timelock)
        .config(config(&f.config))
        .instruction();
    f.ok(&same(generated, f.create_vault_ix()));
    assert_vault(f);
    c.f.ok(&c.initialize_ix(c.f.owner));
    let f = &mut c.f;

    let deposit_permit = borsh::to_vec(&permit(f, PermitKind::Deposit, &f.user, 0)).unwrap();
    let generated = DepositBuilder::new()
        .depositor(f.user)
        .vault(f.vault)
        .asset_mint(f.asset_mint)
        .share_mint(f.share_mint)
        .idle_account(f.idle_account)
        .depositor_assets(f.user_assets)
        .depositor_shares(f.user_shares)
        .asset_token_program(f.token_program)
        .assets(1_000)
        .min_shares_out(1_000)
        .permit(deposit_permit.clone().into())
        .instruction();
    f.ok(&same(
        generated,
        f.deposit_with_permit_ix(1_000, 1_000, &deposit_permit),
    ));

    let generated = AllocateBuilder::new()
        .manager(f.manager)
        .vault(f.vault)
        .idle_account(f.idle_account)
        .amount(600)
        .data(vec![].into())
        .add_remaining_accounts(&adapter("deposit"))
        .instruction();
    f.ok(&same(generated, f.allocate_ix(600)));

    // The oracle's signed price is `Simulate`'s opaque data: 660 for 600 units.
    c.f.advance(10, YEAR / 2);
    let by_hand = c.price_ix(&oracle(), PRICE_SCALE / 10 * 11, YEAR);
    let f = &mut c.f;
    let generated = SimulateBuilder::new()
        .vault(f.vault)
        .idle_account(f.idle_account)
        .data(by_hand.data[1..].to_vec().into())
        .add_remaining_accounts(&adapter("simulate"))
        .instruction();
    f.ok(&same(generated, by_hand));
    // A gain under an unlock period: the lock fields are in use. Half the
    // period later half of it has unlocked and earned its fee.
    assert_ne!((f.state().locked(), f.state().unlock_end()), (0, 0));
    assert_vault(f);
    f.advance(1, 43_200);
    f.reprice();
    assert_ne!((f.state().locked(), f.state().fee_shares()), (0, 0));
    assert_vault(f);

    let generated = CollectFeesBuilder::new()
        .vault(f.vault)
        .share_mint(f.share_mint)
        .recipient_shares(f.fee_shares)
        .instruction();
    f.ok(&same(generated, f.collect_fees_ix()));
    assert_ne!(f.balance(&f.fee_shares), 0);

    // The custodian returns what the fulfilment will pull in.
    c.send_back(500);
    let f = &mut c.f;
    f.ok(&request_redeem(f, 800, 1));
    assert_ticket(f, 1);
    assert_ne!(f.state().tickets(), 0);
    assert_vault(f);
    let nonce = Ticket::from_bytes(&f.account(&f.ticket(1)).data)
        .unwrap()
        .nonce;
    let fulfil_permit = permit(f, PermitKind::Fulfil, &f.ticket(1), nonce);
    let by_hand = f.fulfil_with_permit_ix(1, &borsh::to_vec(&fulfil_permit).unwrap());
    let generated = FulfilBuilder::new()
        .vault(f.vault)
        .ticket(f.ticket(1))
        .payer(f.payer)
        .escrow_account(f.escrow_account)
        .share_mint(f.share_mint)
        .idle_account(f.idle_account)
        .asset_mint(f.asset_mint)
        .destination(f.user_assets)
        .asset_token_program(f.token_program)
        .permit(fulfil_permit)
        .data(vec![].into())
        .add_remaining_accounts(&adapter("withdraw"))
        .instruction();
    f.ok(&same(generated, by_hand));
    assert_eq!(f.ticket_shares(1), None);

    f.ok(&request_redeem(f, 100, 2));
    let generated = CancelRedeemBuilder::new()
        .owner(f.user)
        .payer(f.payer)
        .vault(f.vault)
        .ticket(f.ticket(2))
        .share_mint(f.share_mint)
        .escrow_account(f.escrow_account)
        .owner_shares(f.user_shares)
        .instruction();
    f.ok(&same(generated, f.cancel_redeem_ix(2)));
    assert_eq!(f.ticket_shares(2), None);

    c.send_back(100);
    let f = &mut c.f;
    let generated = DeallocateBuilder::new()
        .manager(f.manager)
        .vault(f.vault)
        .idle_account(f.idle_account)
        .amount(100)
        .data(vec![].into())
        .add_remaining_accounts(&adapter("withdraw"))
        .instruction();
    f.ok(&same(generated, f.deallocate_ix(100)));

    // The role instructions. A pending configuration that differs from the
    // active one in every field, then a pending owner, so that no two fields
    // of the vault can be swapped unnoticed.
    let pending = ConfigArgs {
        manager: f.user,
        guardian: f.owner,
        fee_recipient: f.manager,
        deposit_authority: f.fee_recipient,
        withdraw_authority: f.stranger,
        debt_cap: 1 << 40,
        deposit_cap: 1 << 41,
        max_age: 5,
        fulfil_delay: 3_600,
        unlock_period: 7_200,
        performance_fee_bps: 2_000,
        management_fee_bps: 100,
    };
    let generated = SubmitConfigBuilder::new()
        .authority(f.owner)
        .vault(f.vault)
        .config(config(&pending))
        .instruction();
    f.ok(&same(generated, f.submit_config_ix(f.owner, &pending)));

    let generated = SetPausedBuilder::new()
        .authority(f.guardian)
        .vault(f.vault)
        .paused(true)
        .instruction();
    f.ok(&same(generated, f.set_paused_ix(f.guardian, true)));

    let generated = TransferOwnershipBuilder::new()
        .authority(f.owner)
        .vault(f.vault)
        .new_owner(f.stranger)
        .instruction();
    f.ok(&same(
        generated,
        f.transfer_ownership_ix(f.owner, f.stranger),
    ));
    assert_vault(f);

    let generated = AcceptOwnershipBuilder::new()
        .authority(f.stranger)
        .vault(f.vault)
        .instruction();
    f.ok(&same(generated, f.accept_ownership_ix(f.stranger)));

    f.advance(1, f.timelock as i64);
    let generated = ExecuteConfigBuilder::new()
        .authority(f.stranger)
        .vault(f.vault)
        .instruction();
    f.ok(&same(generated, f.execute_config_ix(f.stranger)));

    let generated = WindDownBuilder::new()
        .authority(f.stranger)
        .vault(f.vault)
        .instruction();
    f.ok(&same(generated, f.wind_down_ix(f.stranger)));

    // The executed configuration made the old owner the guardian.
    let generated = WriteOffBuilder::new()
        .authority(f.owner)
        .vault(f.vault)
        .value(1)
        .instruction();
    f.ok(&same(generated, f.write_off_ix(f.owner, 1)));
    assert_vault(f);
}

/// An ungated vault whose share mint is Token-2022: the share token program
/// is overridden, and a request idle covers is paid at once, with no ticket.
#[test]
fn token_2022_shares_and_a_request_paid_on_the_spot() {
    let mut c = Custodied::setup_with_shares(TOKEN, TOKEN_2022);
    c.open();
    let f = &mut c.f;
    assert_eq!(f.share_program, TOKEN_2022);

    let generated = DepositBuilder::new()
        .depositor(f.user)
        .vault(f.vault)
        .asset_mint(f.asset_mint)
        .share_mint(f.share_mint)
        .idle_account(f.idle_account)
        .depositor_assets(f.user_assets)
        .depositor_shares(f.user_shares)
        .asset_token_program(f.token_program)
        .share_token_program(TOKEN_2022)
        .assets(1_000)
        .min_shares_out(1_000)
        .permit(vec![].into())
        .instruction();
    f.ok(&same(generated, f.deposit_ix(1_000, 1_000)));

    f.ok(&request_redeem(f, 400, 1));
    assert_eq!(f.ticket_shares(1), None);
    assert_eq!(f.balance(&f.escrow_account), 0);
    assert_eq!(f.balance(&f.user_shares), 600);
    assert_eq!(f.balance(&f.user_assets), USER_BALANCE - 600);
}

#[test]
fn generated_views_answer_like_the_program_methods() {
    let mut c = Custodied::new();
    c.f.ok(&c.f.deposit_ix(1_000, 0));
    c.f.ok(&c.f.allocate_ix(500));
    c.f.advance(1, YEAR);
    c.value(1_000);
    let mut f = c.f;

    let answer = |f: &mut Fixture, ix: Instruction| {
        u64::from_le_bytes(f.ok(&ix).return_data.as_slice().try_into().unwrap())
    };
    let ix = ConvertToSharesBuilder::new()
        .vault(f.vault)
        .assets(300)
        .instruction();
    assert_eq!(answer(&mut f, ix), f.state().to_shares(300).unwrap());
    let ix = ConvertToAssetsBuilder::new()
        .vault(f.vault)
        .shares(300)
        .instruction();
    assert_eq!(answer(&mut f, ix), f.state().to_assets(300).unwrap());
    let ix = MaxDepositBuilder::new().vault(f.vault).instruction();
    assert_eq!(answer(&mut f, ix), f.state().max_deposit().unwrap());
}

#[test]
fn error_codes_match() {
    macro_rules! assert_codes {
        ($($name:ident),*) => {
            $(assert_eq!(SolanaVaultError::$name as u32, VaultError::$name as u32);)*
        };
    }
    assert_codes!(
        NotMutable,
        NotSigner,
        InvalidAccountOwner,
        InvalidAccountLength,
        InvalidVersion,
        AlreadyInitialized,
        InvalidSeeds,
        InvalidEventAuthority,
        InvalidAuthority,
        InvalidConfig,
        TimelockNotPassed,
        InvalidStatus,
        InvalidMint,
        InvalidTokenAccount,
        InvalidAdapter,
        InvalidReturnData,
        AdapterCallTooLarge,
        StaleReport,
        ZeroAmount,
        AmountTooLarge,
        SlippageExceeded,
        InvalidTicket,
        InvalidPermit,
        PermitExpired,
        NothingToFulfil,
        MathOverflow,
        Worthless
    );
}
