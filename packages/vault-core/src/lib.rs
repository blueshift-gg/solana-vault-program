//! Shared account layouts, error codes and value math for the program and its clients.
#![no_std]

pub mod constants;
pub mod errors;
pub mod math;
pub mod state;

/// The message a permit signs: the domain, this program, the kind of permit,
/// the vault, the subject, a nonce and the expiry as a Unix timestamp in
/// seconds. Bound to all of them, a permit is good for nothing else. The
/// nonce is the ticket's for a fulfil permit and zero for a deposit permit,
/// which is meant to be used again until it expires.
pub fn permit_message(
    kind: u8,
    vault: &[u8; 32],
    subject: &[u8; 32],
    nonce: u64,
    expires_at: i64,
) -> [u8; 129] {
    let mut message = [0u8; 129];
    message[..16].copy_from_slice(constants::PERMIT_DOMAIN);
    message[16..48].copy_from_slice(&ID);
    message[48] = kind;
    message[49..81].copy_from_slice(vault);
    message[81..113].copy_from_slice(subject);
    message[113..121].copy_from_slice(&nonce.to_le_bytes());
    message[121..].copy_from_slice(&expires_at.to_le_bytes());
    message
}

// 5Bwdadmxt9EbZyKqAN8FspPGDWbxBdWc929LJwo2mMYn
pub const ID: [u8; 32] = [
    62, 60, 83, 176, 127, 76, 244, 148, 9, 112, 95, 210, 177, 143, 101, 67, 41, 243, 95, 220, 233,
    199, 144, 103, 121, 143, 127, 130, 243, 173, 183, 179,
];
