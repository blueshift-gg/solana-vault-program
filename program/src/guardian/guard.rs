use crate::constants::{VAULT_ACTIVE, VAULT_PAUSED};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{check_role, clock};
use crate::owner::RoleAccounts;
use crate::state::{Load, Vault};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # SetPaused
///
/// Stop new money while something is looked into, or let it in again. A pause
/// blocks deposits and allocations and nothing else: reports, deallocations
/// and every exit keep working, so a pause can never trap a holder.
///
/// Accounts: see `RoleAccounts`; the authority is the guardian.
///
/// Parameters:
/// 1. paused: u8,                  // 1 to pause, 0 to resume
///
/// Event Data:
/// - discriminator: u8, (255u8, 10u8)
/// - vault: Pubkey,
/// - status: u8,
pub struct SetPaused<'a> {
    pub accounts: RoleAccounts<'a>,
    pub paused: bool,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for SetPaused<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("SetPaused");

        let accounts = RoleAccounts::try_from(accounts)?;
        let paused = match data {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidInstructionData),
        };

        Ok(Self { accounts, paused })
    }
}

impl<'a> SetPaused<'a> {
    pub const DISCRIMINATOR: &'a u8 = &10;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.config.guardian())?;

        // Only between active and paused: a wind-down is never resumed
        let (from, to) = if self.paused {
            (VAULT_ACTIVE, VAULT_PAUSED)
        } else {
            (VAULT_PAUSED, VAULT_ACTIVE)
        };
        if vault.status() != from {
            return Err(VaultError::InvalidStatus.into());
        }
        vault.set_status(to);

        // Log the SetPaused Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), &[to]],
        )
    }
}

/// # WriteOff
///
/// Recognise a loss. Part of what the strategy held is gone, and no report
/// will say so: an adapter that no longer answers, a protocol that was
/// drained, funds off chain that will not come back. The guardian sets the
/// strategy's value down to what is still there, and the share price falls to
/// match at once.
///
/// With the strategy written down to zero the vault has nothing left to
/// reprice, so holders can exit against idle with no report at all. Funds the
/// strategy returns afterwards are not lost: they wait in the strategy
/// account, `Simulate` counts them back in as a gain, locked like any other,
/// and `Deallocate` brings home what has been counted.
///
/// Accounts: see `RoleAccounts`; the authority is the guardian.
///
/// Parameters:
/// 1. value: u64,                  // what the strategy is still worth
///
/// Instruction Checks:
/// - Value: at most the current debt; that needs the vault, so it runs in process
///
/// Event Data:
/// - discriminator: u8, (255u8, 11u8)
/// - vault: Pubkey,
/// - value: u64,
pub struct WriteOff<'a> {
    pub accounts: RoleAccounts<'a>,
    pub value: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for WriteOff<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("WriteOff");

        let accounts = RoleAccounts::try_from(accounts)?;
        let value = u64::from_le_bytes(
            data.try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        );

        Ok(Self { accounts, value })
    }
}

impl<'a> WriteOff<'a> {
    pub const DISCRIMINATOR: &'a u8 = &11;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.config.guardian())?;
        let (_, now) = clock()?;
        vault.write_off(self.value, now)?;

        // Log the WriteOff Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), &self.value.to_le_bytes()],
        )
    }
}
