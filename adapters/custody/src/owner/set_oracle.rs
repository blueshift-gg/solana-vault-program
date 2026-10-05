use crate::constants::SET_ORACLE;
use crate::errors::CustodyError;
use crate::helpers::{load_vault, now};
use crate::state::Custody;
use pinocchio::log::sol_log;
use pinocchio::pubkey::Pubkey;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # SetOracle
///
/// Name a new oracle. It takes over after the vault's own timelock, the same
/// wait every other change to the vault sits out, so holders can leave before
/// someone else starts pricing them. Naming another replaces a pending one.
///
/// Accounts:
///
/// 1. owner:               [signer]        the vault's owner
/// 2. vault:
/// 3. custody:             [mut]
///
/// Parameters:
/// 1. oracle: Pubkey,
///
/// Account Checks:
/// - Owner: signer, the vault's owner
/// - Vault: a vault of the vault program whose adapter is this program
/// - Custody: writable, deserialized, of this vault
pub struct SetOracle<'a> {
    pub owner: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub custody: &'a AccountInfo,
    pub oracle: &'a Pubkey,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for SetOracle<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("SetOracle");

        let [owner, vault, custody] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !owner.is_signer() {
            return Err(CustodyError::NotSigner.into());
        }
        if load_vault(vault)?.owner().ne(owner.key()) {
            return Err(CustodyError::InvalidOwner.into());
        }
        if Custody::load(custody)?.vault().ne(vault.key()) {
            return Err(CustodyError::InvalidCustody.into());
        }
        let oracle = data
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        // Return the initialized struct
        Ok(Self {
            owner,
            vault,
            custody,
            oracle,
        })
    }
}

impl<'a> SetOracle<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &SET_ORACLE;

    pub fn process(&mut self) -> ProgramResult {
        let now = now()?;
        let takes_over = i64::try_from(load_vault(self.vault)?.timelock())
            .ok()
            .and_then(|timelock| now.checked_add(timelock))
            .ok_or(CustodyError::MathOverflow)?;
        let custody = Custody::load_mut(self.custody)?;
        // An oracle whose wait is already over takes its place first, so
        // naming another does not hand pricing back to the one it replaced
        custody.promote_oracle(now);
        custody.set_pending_oracle(self.oracle, takes_over);
        Ok(())
    }
}
