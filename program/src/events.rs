//! One event per instruction, emitted through a CPI to this program signed by
//! the event authority PDA. The `EmitEvent` instruction accepts only that signer,
//! so events live in inner instructions and no other program can forge them.
//! The CPI needs the program's own account, so every instruction carries
//! `event_authority` and `program` after its own accounts; the CPI itself
//! rejects wrong ones, so handlers do not check them.
//!
//! Wire layout: `[EVENT_DISCRIMINATOR, instruction discriminator, fields in order]`.
//! Each handler documents its fields under "Event Data".

use crate::constants::{
    EVENT_AUTHORITY, EVENT_AUTHORITY_BUMP, EVENT_AUTHORITY_SEED, EVENT_DISCRIMINATOR,
};
use crate::errors::VaultError;
use pinocchio::{
    account_info::AccountInfo,
    cpi::invoke_signed,
    instruction::{AccountMeta, Instruction, Seed, Signer},
    program_error::ProgramError,
    ProgramResult,
};

/// The largest event: the two discriminators and four keys.
const MAX_EVENT_LEN: usize = 2 + 4 * 32;

/// The instruction every event CPI targets. It does nothing; the inner
/// instruction's data is the event. Only the event authority PDA can sign it,
/// and only this program can sign for that PDA.
pub fn emit_event(accounts: &[AccountInfo]) -> ProgramResult {
    let [event_authority, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !event_authority.is_signer() || event_authority.key().ne(&EVENT_AUTHORITY) {
        return Err(VaultError::InvalidEventAuthority.into());
    }
    Ok(())
}

/// Emit `instruction`'s event: its fields, concatenated in order.
#[inline(always)]
pub fn emit(
    event_authority: &AccountInfo,
    program: &AccountInfo,
    instruction: &u8,
    fields: &[&[u8]],
) -> ProgramResult {
    let mut data = [0u8; MAX_EVENT_LEN];
    data[0] = EVENT_DISCRIMINATOR;
    data[1] = *instruction;
    let mut len = 2;
    for field in fields {
        data[len..len + field.len()].copy_from_slice(field);
        len += field.len();
    }

    let bump = [EVENT_AUTHORITY_BUMP];
    let seeds = [Seed::from(EVENT_AUTHORITY_SEED), Seed::from(&bump)];
    invoke_signed(
        &Instruction {
            program_id: &crate::ID,
            accounts: &[
                AccountMeta::readonly_signer(&EVENT_AUTHORITY),
                AccountMeta::readonly(&crate::ID),
            ],
            data: &data[..len],
        },
        &[event_authority, program],
        &[Signer::from(&seeds)],
    )
}
