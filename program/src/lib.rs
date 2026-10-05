// Host builds (unit tests, clippy) compile the handlers without an entrypoint
// that calls them; silence the resulting dead-code noise there only.
#![cfg_attr(not(target_os = "solana"), allow(dead_code, unused_imports))]

use pinocchio::{
    account_info::AccountInfo, default_panic_handler, no_allocator, program_entrypoint,
    program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

// The program never allocates; `no_allocator!` turns an accidental heap use
// into a hard failure.
program_entrypoint!(process_instruction);
no_allocator!();
default_panic_handler!();

pub mod owner;
pub use owner::*;

pub mod guardian;
pub use guardian::*;

pub mod manager;
pub use manager::*;

pub mod user;
pub use user::*;

pub mod keeper;
pub use keeper::*;

pub mod adapter;
pub mod events;
pub mod helpers;
pub mod state;
pub mod views;

pub use vault_core::{constants, errors, math, ID};

fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    // Views - the read interface, eight-byte discriminators. Their first
    // bytes match no instruction below. The answer is the return data.
    match instruction_data.split_first_chunk() {
        Some((&constants::VIEW_CONVERT_TO_SHARES, data)) => {
            return views::convert_to_shares(accounts, data)
        }
        Some((&constants::VIEW_CONVERT_TO_ASSETS, data)) => {
            return views::convert_to_assets(accounts, data)
        }
        Some((&constants::VIEW_MAX_DEPOSIT, _)) => return views::max_deposit(accounts),
        _ => {}
    }

    match instruction_data.split_first() {
        // Keeper Instructions - Discriminators from 40. Simulate first: it precedes every priced operation.
        Some((Simulate::DISCRIMINATOR, data)) => Simulate::try_from((data, accounts))?.process(),
        Some((Fulfil::DISCRIMINATOR, data)) => Fulfil::try_from((data, accounts))?.process(),
        Some((CollectFees::DISCRIMINATOR, _)) => CollectFees::try_from(accounts)?.process(),

        // User Instructions - Discriminators from 30
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),
        Some((RequestRedeem::DISCRIMINATOR, data)) => {
            RequestRedeem::try_from((data, accounts))?.process()
        }
        Some((CancelRedeem::DISCRIMINATOR, _)) => CancelRedeem::try_from(accounts)?.process(),

        // Manager Instructions - Discriminators from 20
        Some((Allocate::DISCRIMINATOR, data)) => Allocate::try_from((data, accounts))?.process(),
        Some((Deallocate::DISCRIMINATOR, data)) => {
            Deallocate::try_from((data, accounts))?.process()
        }

        // Guardian Instructions - Discriminators from 10
        Some((SetPaused::DISCRIMINATOR, data)) => SetPaused::try_from((data, accounts))?.process(),
        Some((WriteOff::DISCRIMINATOR, data)) => WriteOff::try_from((data, accounts))?.process(),

        // Owner Instructions - Discriminators from 0
        Some((CreateVault::DISCRIMINATOR, data)) => {
            CreateVault::try_from((data, accounts))?.process()
        }
        Some((SubmitConfig::DISCRIMINATOR, data)) => {
            SubmitConfig::try_from((data, accounts))?.process()
        }
        Some((ExecuteConfig::DISCRIMINATOR, _)) => ExecuteConfig::try_from(accounts)?.process(),
        Some((TransferOwnership::DISCRIMINATOR, data)) => {
            TransferOwnership::try_from((data, accounts))?.process()
        }
        Some((AcceptOwnership::DISCRIMINATOR, _)) => AcceptOwnership::try_from(accounts)?.process(),
        Some((WindDown::DISCRIMINATOR, _)) => WindDown::try_from(accounts)?.process(),

        // Self-CPI EmitEvent - Discriminator 255
        Some((&constants::EVENT_DISCRIMINATOR, _)) => events::emit_event(accounts),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
