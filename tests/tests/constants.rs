//! Every constant transcribed from elsewhere, checked against its source.

use solana_pubkey::Pubkey;
use spl_token_2022_interface::extension::ExtensionType;
use vault_core::constants::*;
use vault_tests::PROGRAM_ID;

#[test]
fn event_authority_is_the_derived_pda() {
    let (address, bump) = Pubkey::find_program_address(&[EVENT_AUTHORITY_SEED], &PROGRAM_ID);
    assert_eq!(address.to_bytes(), EVENT_AUTHORITY);
    assert_eq!(bump, EVENT_AUTHORITY_BUMP);
}

#[test]
fn adapter_discriminators_are_namespaced_hashes() {
    for (name, discriminator) in [
        ("simulate", ADAPTER_SIMULATE),
        ("deposit", ADAPTER_DEPOSIT),
        ("withdraw", ADAPTER_WITHDRAW),
    ] {
        let hash = solana_sha256_hasher::hash(format!("solana-vault-adapter:{name}").as_bytes());
        assert_eq!(hash.to_bytes()[..8], discriminator);
    }
}

#[test]
fn view_discriminators_are_namespaced_hashes() {
    for (name, discriminator) in [
        ("convert_to_shares", VIEW_CONVERT_TO_SHARES),
        ("convert_to_assets", VIEW_CONVERT_TO_ASSETS),
        ("max_deposit", VIEW_MAX_DEPOSIT),
    ] {
        let hash = solana_sha256_hasher::hash(format!("solana-vault-interface:{name}").as_bytes());
        assert_eq!(hash.to_bytes()[..8], discriminator);
    }
}

#[test]
fn token_2022_constants_match_the_program() {
    assert_eq!(TOKEN_2022, vault_tests::TOKEN_2022.to_bytes());
    assert_eq!(EXTENSION_TRANSFER_HOOK, ExtensionType::TransferHook as u16);
    assert_eq!(EXTENSION_MEMO_TRANSFER, ExtensionType::MemoTransfer as u16);
}

#[test]
fn token_layout_offsets_match_the_program() {
    use solana_program_option::COption;
    use solana_program_pack::Pack;
    use spl_token_2022_interface::{
        extension::AccountType,
        state::{Account, AccountState, Mint},
    };

    let some = |data: &[u8], at: usize| data[at..at + 4] == [1, 0, 0, 0];

    let account = Account {
        mint: Pubkey::new_unique(),
        owner: Pubkey::new_unique(),
        amount: 0x0102_0304_0506_0708,
        delegate: COption::Some(Pubkey::new_unique()),
        state: AccountState::Frozen,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::Some(Pubkey::new_unique()),
    };
    let mut data = [0; Account::LEN];
    account.pack_into_slice(&mut data);
    assert_eq!(data.len(), TOKEN_ACCOUNT_LEN);
    assert_eq!(data[..32], account.mint.to_bytes());
    assert_eq!(data[TOKEN_ACCOUNT_OWNER..][..32], account.owner.to_bytes());
    assert_eq!(
        data[TOKEN_ACCOUNT_AMOUNT..][..8],
        account.amount.to_le_bytes()
    );
    assert_eq!(data[TOKEN_ACCOUNT_STATE], AccountState::Frozen as u8);
    assert!(some(&data, TOKEN_ACCOUNT_DELEGATE));
    assert!(some(&data, TOKEN_ACCOUNT_CLOSE_AUTHORITY));
    // Each tag is read alone: clearing one option leaves the other set.
    for (cleared, kept) in [
        (TOKEN_ACCOUNT_DELEGATE, TOKEN_ACCOUNT_CLOSE_AUTHORITY),
        (TOKEN_ACCOUNT_CLOSE_AUTHORITY, TOKEN_ACCOUNT_DELEGATE),
    ] {
        let mut account = account;
        if cleared == TOKEN_ACCOUNT_DELEGATE {
            account.delegate = COption::None;
        } else {
            account.close_authority = COption::None;
        }
        account.pack_into_slice(&mut data);
        assert!(!some(&data, cleared) && some(&data, kept));
    }

    let mint = Mint {
        mint_authority: COption::Some(Pubkey::new_unique()),
        supply: 0,
        decimals: 7,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    let mut data = [0; Mint::LEN];
    mint.pack_into_slice(&mut data);
    assert_eq!(data.len(), MINT_LEN);
    assert!(some(&data, MINT_AUTHORITY));
    assert_eq!(
        data[MINT_AUTHORITY + 4..][..32],
        mint.mint_authority.unwrap().to_bytes()
    );
    assert_eq!(data[MINT_DECIMALS], 7);
    assert_eq!(data[MINT_IS_INITIALIZED], 1);

    assert_eq!(ACCOUNT_TYPE, Account::LEN);
    assert_eq!(ACCOUNT_TYPE_MINT, AccountType::Mint as u8);
    assert_eq!(ACCOUNT_TYPE_TOKEN_ACCOUNT, AccountType::Account as u8);
}
