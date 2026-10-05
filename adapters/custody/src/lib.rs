//! Vault adapter for funds managed off chain.
//!
//! Some strategies cannot be read by a program: a treasury bill, a loan, a
//! position at an exchange. This adapter gives such a vault the same three
//! instructions as any other, by keeping the books the way a fund does:
//!
//! - **Units and a price.** What is off chain is counted in units. Funds
//!   going out buy units at the current price; funds coming back redeem them.
//!   An oracle signs the price of one unit, off chain, and anyone delivers
//!   it. The position is worth `units × price`. Because flows and price are
//!   separate numbers, a price signed before a flow is still right after it.
//!   The oracle computes its price as the off-chain value divided by `units`
//!   as stored here, so its books and these cannot drift apart. A report
//!   names the book it prices; when the book empties and a new one starts,
//!   at par, reports signed for the old one no longer apply.
//! - **One way out.** `Deposit` sends funds to the custody account named at
//!   setup, and nowhere else. The vault's manager decides when; it cannot
//!   decide where.
//! - **One way back.** The custodian returns funds with a plain transfer to
//!   the return account, which this adapter owns. `Withdraw` settles what
//!   arrived against units and hands it to the vault. Nothing rests here: the
//!   vault's idle account is the only liquidity buffer.
//!
//! A price is good until its expiry. Past it the adapter stops answering, so
//! the vault goes stale and stops pricing deposits and redemptions until the
//! oracle signs again.

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

pub mod owner;
pub use owner::*;

pub mod vault;
pub use vault::*;

pub mod constants;
pub mod errors;
pub mod helpers;
pub mod state;

// PRBu3zLbyGKYfzkmA7dsVfLcfLwtz9bgGBoVnSgGHK9
pub const ID: Pubkey = [
    5, 190, 32, 139, 160, 214, 160, 21, 27, 27, 166, 189, 204, 235, 90, 149, 155, 69, 67, 80, 231,
    242, 69, 139, 233, 203, 113, 62, 83, 111, 190, 4,
];

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first_chunk() {
        // Vault Instructions - the adapter interface, called by the vault program
        Some((Simulate::DISCRIMINATOR, data)) => Simulate::try_from((data, accounts))?.process(),
        Some((Withdraw::DISCRIMINATOR, data)) => Withdraw::try_from((data, accounts))?.process(),
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),

        // Owner Instructions - this program's own
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((data, accounts))?.process()
        }
        Some((SetOracle::DISCRIMINATOR, data)) => SetOracle::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
