//! The read interface (see `constants`): one account, the vault, and the
//! answer as a `u64` LE in return data, readable by CPI or by simulation.
//! A deposit is priced against everything counted and a redemption against
//! what has unlocked, so the two conversions differ while gains are locked.
//! Views answer from stored values and do not check freshness; a caller that
//! needs a fresh price runs `Simulate` first, exactly as before a deposit.

use crate::state::{Load, Vault};
use pinocchio::cpi::set_return_data;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

fn vault(accounts: &[AccountInfo]) -> Result<&Vault, ProgramError> {
    let [vault, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    Vault::load(vault)
}

fn amount(data: &[u8]) -> Result<u64, ProgramError> {
    Ok(u64::from_le_bytes(
        data.try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    ))
}

/// Shares a deposit of `assets` would mint.
pub fn convert_to_shares(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let shares = vault(accounts)?.to_shares(amount(data)?)?;
    set_return_data(&shares.to_le_bytes());
    Ok(())
}

/// Assets a ticket of `shares` would be paid.
pub fn convert_to_assets(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let assets = vault(accounts)?.to_assets(amount(data)?)?;
    set_return_data(&assets.to_le_bytes());
    Ok(())
}

/// The largest deposit the vault accepts now; zero unless it is active.
pub fn max_deposit(accounts: &[AccountInfo]) -> ProgramResult {
    let max = vault(accounts)?.max_deposit()?;
    set_return_data(&max.to_le_bytes());
    Ok(())
}
