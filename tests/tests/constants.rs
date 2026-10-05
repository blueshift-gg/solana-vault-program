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
