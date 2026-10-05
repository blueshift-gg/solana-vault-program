//! Program-owned account views. Every `unsafe` needed to overlay a layout on
//! account bytes lives here; handlers only see checked views.

use crate::{constants::*, errors::VaultError};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};
pub use vault_core::state::{Config, Ticket, Vault};

pub trait Load: Sized {
    /// View the account after checking owner, length and version. The program
    /// never holds a checked borrow, so callers must not alias a mutable view
    /// of the same account.
    fn load(account: &AccountInfo) -> Result<&Self, ProgramError>;

    /// Same checks as `load`, returning a mutable view.
    #[allow(clippy::mut_from_ref)]
    fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError>;

    /// View an account this instruction just created: program-owned, exactly
    /// sized and still zeroed, so writing every field is the only valid next step.
    #[allow(clippy::mut_from_ref)]
    fn load_new(account: &AccountInfo) -> Result<&mut Self, ProgramError>;
}

macro_rules! load {
    ($name:ident, $len:expr, $version:expr) => {
        impl Load for $name {
            #[inline(always)]
            fn load(account: &AccountInfo) -> Result<&Self, ProgramError> {
                if !account.is_owned_by(&crate::ID) {
                    return Err(VaultError::InvalidAccountOwner.into());
                }
                if account.data_len() != $len {
                    return Err(VaultError::InvalidAccountLength.into());
                }
                // SAFETY: length checked above; all fields have alignment 1.
                let this = unsafe { $name::from_bytes_unchecked(account.borrow_data_unchecked()) };
                if this.version() != $version {
                    return Err(VaultError::InvalidVersion.into());
                }
                Ok(this)
            }

            #[inline(always)]
            fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
                Self::load(account)?;
                if !account.is_writable() {
                    return Err(VaultError::NotMutable.into());
                }
                // SAFETY: same layout guarantees as `load`; the caller holds no other view.
                Ok(unsafe { $name::from_bytes_unchecked_mut(account.borrow_mut_data_unchecked()) })
            }

            #[inline(always)]
            fn load_new(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
                if !account.is_owned_by(&crate::ID) {
                    return Err(VaultError::InvalidAccountOwner.into());
                }
                if account.data_len() != $len {
                    return Err(VaultError::InvalidAccountLength.into());
                }
                // SAFETY: length checked above; all fields have alignment 1.
                let this =
                    unsafe { $name::from_bytes_unchecked_mut(account.borrow_mut_data_unchecked()) };
                if this.version() != 0 {
                    return Err(VaultError::AlreadyInitialized.into());
                }
                Ok(this)
            }
        }
    };
}

load!(Vault, VAULT_LEN, VAULT_VERSION);
load!(Ticket, TICKET_LEN, TICKET_VERSION);
