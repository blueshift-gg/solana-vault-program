use super::BookAccounts;
use crate::helpers::{now, transfer, TokenAccount};
use crate::state::Custody;
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_WITHDRAW;

/// # Withdraw
///
/// Bring home what the custodian sent back. The custodian returns funds with
/// a plain transfer to the return account; this settles whatever arrived
/// since last time against units, at the current price, and hands the vault
/// up to the amount it asked for. Nothing sent back is nothing to do.
///
/// > Redeem units for what arrived in the return account
/// > Transfer up to `amount` to the strategy account, signed by the custody
///
/// Accounts: see `BookAccounts`.
///
/// Parameters:
/// 1. amount: u64,
pub struct Withdraw<'a> {
    pub accounts: BookAccounts<'a>,
    pub amount: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Withdraw");

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

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_WITHDRAW;

    pub fn process(&mut self) -> ProgramResult {
        let custody = Custody::load_mut(self.accounts.custody)?;
        let price = custody.live_price(now()?)?;
        let held = TokenAccount::load(
            self.accounts.return_account,
            self.accounts.token_program.key(),
        )?
        .amount;
        let amount = custody.redeem(held, self.amount, price)?;
        if amount == 0 {
            return Ok(());
        }

        let seeds = custody.signer_seeds();
        let seeds = seeds.as_seeds();
        transfer(
            self.accounts.token_program,
            self.accounts.return_account,
            self.accounts.asset_mint,
            self.accounts.strategy_account,
            self.accounts.custody,
            amount,
            &[Signer::from(&seeds)],
        )
    }
}
