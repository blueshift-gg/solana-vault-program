use super::RoleAccounts;
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{check_role, clock};
use crate::state::{Config, Load, Vault};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # SubmitConfig
///
/// Propose a new configuration. It replaces any pending one and becomes
/// executable once the vault's timelock has passed, so holders can leave
/// before roles, fees, limits or the withdrawal authority change.
///
/// Accounts: see `RoleAccounts`; the authority is the owner.
///
/// Parameters:
/// 1. config: Config,
///
/// Instruction Checks:
/// - Config: fees and fulfil delay at or below their ceilings
///
/// Event Data:
/// - discriminator: u8, (255u8, 1u8)
/// - vault: Pubkey,
/// - executable_at: i64,
pub struct SubmitConfig<'a> {
    pub accounts: RoleAccounts<'a>,
    pub config: &'a Config,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for SubmitConfig<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("SubmitConfig");

        let accounts = RoleAccounts::try_from(accounts)?;
        let config = Config::from_bytes(data)?;

        Ok(Self { accounts, config })
    }
}

impl<'a> SubmitConfig<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.owner())?;

        let (_, now) = clock()?;
        let executable_at = i64::try_from(vault.timelock())
            .ok()
            .and_then(|timelock| now.checked_add(timelock))
            .ok_or(VaultError::InvalidConfig)?;
        vault.pending = *self.config;
        vault.set_pending_at(executable_at);

        // Log the SubmitConfig Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), &executable_at.to_le_bytes()],
        )
    }
}

/// # ExecuteConfig
///
/// Apply the pending configuration once its timelock has passed. Gains and
/// fees are brought up to date under the old one first, so a new fee never
/// applies to time before it.
///
/// Accounts: see `RoleAccounts`; the authority is the owner.
///
/// Event Data:
/// - discriminator: u8, (255u8, 2u8)
/// - vault: Pubkey,
pub struct ExecuteConfig<'a> {
    pub accounts: RoleAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for ExecuteConfig<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("ExecuteConfig");

        Ok(Self {
            accounts: RoleAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> ExecuteConfig<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.accounts.vault)?;
        check_role(self.accounts.authority, vault.owner())?;

        let (_, now) = clock()?;
        if vault.pending_at() == 0 || now < vault.pending_at() {
            return Err(VaultError::TimelockNotPassed.into());
        }
        // Settle everything owed under the old configuration first, so the
        // new one applies from now on and to nothing before
        vault.accrue(now)?;
        vault.config = vault.pending;
        vault.set_pending_at(0);
        vault.set_fee_ts(now);

        // Log the ExecuteConfig Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key()],
        )
    }
}
