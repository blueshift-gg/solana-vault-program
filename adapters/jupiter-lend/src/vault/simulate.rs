use super::PositionAccounts;
use pinocchio::cpi::set_return_data;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_SIMULATE;

/// # Simulate
///
/// Say what the position is worth: the fTokens the strategy authority holds,
/// at the exchange price the market last stored. Reads two accounts and
/// calls nothing.
///
/// Accounts: see `PositionAccounts`.
///
/// Return Data:
/// - value: u64,
pub struct Simulate<'a> {
    pub accounts: PositionAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Simulate<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("Simulate");

        Ok(Self {
            accounts: PositionAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> Simulate<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_SIMULATE;

    pub fn process(&mut self) -> ProgramResult {
        set_return_data(&self.accounts.position.value()?.to_le_bytes());
        Ok(())
    }
}
