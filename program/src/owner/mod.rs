pub mod config;
pub use config::*;

pub mod create_vault;
pub use create_vault::*;

pub mod lifecycle;
pub use lifecycle::*;

use crate::errors::VaultError;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// The accounts of every instruction that only changes vault settings:
///
/// 1. authority:           [signer]        the role the instruction requires
/// 2. vault:               [mut]
/// 3. event_authority:
/// 4. program:             [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Authority: signer; its role needs the vault, so it is checked in process
/// - Vault: writable; deserialized in process
/// - EventAuthority, Program: no need to check since the event CPI fails otherwise
pub struct RoleAccounts<'a> {
    pub authority: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for RoleAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [authority, vault, event_authority, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !authority.is_signer() {
            return Err(VaultError::NotSigner.into());
        }
        if !vault.is_writable() {
            return Err(VaultError::NotMutable.into());
        }

        Ok(Self {
            authority,
            vault,
            event_authority,
            program,
        })
    }
}
