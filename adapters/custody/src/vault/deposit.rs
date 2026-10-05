use super::BookAccounts;
use crate::helpers::{now, transfer};
use crate::state::Custody;
use core::mem::size_of;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_DEPOSIT;

/// # Deposit
///
/// Send funds out to custody. They leave the strategy account for the one
/// destination named at setup, and enter the book as units at the current
/// price, which must not have expired.
///
/// > Buy units at the current price
/// > Transfer to the destination, signed by the strategy authority
///
/// Accounts: see `BookAccounts`.
///
/// Parameters:
/// 1. amount: u64,
pub struct Deposit<'a> {
    pub accounts: BookAccounts<'a>,
    pub amount: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Deposit");

        let accounts = BookAccounts::try_from(accounts)?;
        accounts.check_signed()?;
        // The vault may append opaque bytes; this adapter takes none
        let Some((amount, _)) = data.split_first_chunk::<{ size_of::<u64>() }>() else {
            return Err(ProgramError::InvalidInstructionData);
        };

        Ok(Self {
            accounts,
            amount: u64::from_le_bytes(*amount),
        })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_DEPOSIT;

    pub fn process(&mut self) -> ProgramResult {
        let custody = Custody::load_mut(self.accounts.custody)?;
        let price = custody.live_price(now()?)?;
        custody.buy(self.amount, price)?;

        // The vault forwarded the strategy authority's signature
        transfer(
            self.accounts.token_program,
            self.accounts.strategy_account,
            self.accounts.asset_mint,
            self.accounts.destination,
            self.accounts.strategy_authority,
            self.amount,
            &[],
        )
    }
}
