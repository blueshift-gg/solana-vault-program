//! Vault adapter for Jupiter Lend (the Fluid lending program).
//!
//! A vault's position is the fTokens its strategy authority holds, in that
//! authority's associated token account. The adapter keeps no state of its
//! own: it pins the accounts that decide where value sits and lets the lending
//! program check the rest.
//!
//! Two things a vault must satisfy to use this adapter, both the lending
//! program's own rules: the strategy authority is passed writable, and the
//! vault's strategy account is the strategy authority's associated token
//! account for the asset.

// Host builds (unit tests, clippy) compile the handlers without an entrypoint
// that calls them; silence the resulting dead-code noise there only.
#![cfg_attr(not(target_os = "solana"), allow(dead_code, unused_imports))]

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

// The entrypoint exists on chain only, so tests can link this crate as a library.
#[cfg(target_os = "solana")]
mod entrypoint {
    use super::process_instruction;
    use pinocchio::{default_panic_handler, no_allocator, program_entrypoint};
    program_entrypoint!(process_instruction);
    no_allocator!();
    default_panic_handler!();
}

pub mod vault;
pub use vault::*;

pub mod constants;
pub mod errors;
pub mod state;

// 7T9qpK5R9oFtrXjRfS7r17cozooRDyemkCLPziBUJL18
pub const ID: Pubkey = [
    95, 217, 56, 74, 13, 96, 98, 21, 54, 245, 154, 95, 109, 197, 204, 119, 76, 38, 107, 172, 145,
    111, 34, 249, 29, 238, 154, 115, 62, 73, 142, 11,
];

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first_chunk() {
        // Vault Instructions - the adapter interface, called by the vault program
        Some((Simulate::DISCRIMINATOR, _)) => Simulate::try_from(accounts)?.process(),
        Some((Withdraw::DISCRIMINATOR, data)) => Withdraw::try_from((data, accounts))?.process(),
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
