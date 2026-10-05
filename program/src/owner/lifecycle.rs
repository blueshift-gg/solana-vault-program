use super::RoleAccounts;
use crate::constants::{VAULT_ACTIVE, VAULT_PAUSED, VAULT_WIND_DOWN};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::check_role;
use crate::state::{Load, Vault};
use pinocchio::log::sol_log;
use pinocchio::pubkey::Pubkey;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # TransferOwnership
///
/// Name the next owner. Nothing changes until that key accepts, so a
/// mistyped address cannot lock the vault. All zeros cancels a transfer.
///
/// Accounts: see `RoleAccounts`; the authority is the owner.
///
/// Parameters:
/// 1. new_owner: Pubkey,
///
/// Event Data:
/// - discriminator: u8, (255u8, 3u8)
/// - vault: Pubkey,
/// - new_owner: Pubkey,
pub struct TransferOwnership<'a> {
    pub accounts: RoleAccounts<'a>,
    pub new_owner: &'a Pubkey,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for TransferOwnership<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("TransferOwnership");

        let accounts = RoleAccounts::try_from(accounts)?;
        let new_owner = data
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        Ok(Self {
            accounts,
            new_owner,
        })
    }
}

impl<'a> TransferOwnership<'a> {
    pub const DISCRIMINATOR: &'a u8 = &3;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.owner())?;
        vault.set_pending_owner(*self.new_owner);

        // Log the TransferOwnership Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), self.new_owner],
        )
    }
}

/// # AcceptOwnership
///
/// Become the owner. Only the key the current owner named can sign this.
///
/// Accounts: see `RoleAccounts`; the authority is the pending owner.
///
/// Event Data:
/// - discriminator: u8, (255u8, 4u8)
/// - vault: Pubkey,
/// - owner: Pubkey,
pub struct AcceptOwnership<'a> {
    pub accounts: RoleAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for AcceptOwnership<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("AcceptOwnership");

        Ok(Self {
            accounts: RoleAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> AcceptOwnership<'a> {
    pub const DISCRIMINATOR: &'a u8 = &4;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.pending_owner())?;
        vault.set_owner(*self.accounts.authority.key());
        vault.set_pending_owner([0; 32]);

        // Log the AcceptOwnership Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), self.accounts.authority.key()],
        )
    }
}

/// # WindDown
///
/// Close the vault to new money for good: deposits and allocations stop,
/// while reports, deallocations and every exit keep working.
///
/// Accounts: see `RoleAccounts`; the authority is the owner.
///
/// Event Data:
/// - discriminator: u8, (255u8, 5u8)
/// - vault: Pubkey,
pub struct WindDown<'a> {
    pub accounts: RoleAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for WindDown<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("WindDown");

        Ok(Self {
            accounts: RoleAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> WindDown<'a> {
    pub const DISCRIMINATOR: &'a u8 = &5;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.owner())?;
        if vault.status() != VAULT_ACTIVE && vault.status() != VAULT_PAUSED {
            return Err(VaultError::InvalidStatus.into());
        }
        vault.set_status(VAULT_WIND_DOWN);

        // Log the WindDown Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key()],
        )
    }
}
