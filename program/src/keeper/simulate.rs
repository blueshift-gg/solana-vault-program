use crate::adapter::Adapter;
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{clock, TokenAccount};
use crate::state::{Load, Vault};
use pinocchio::cpi::set_return_data;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Simulate
///
/// Reprice the vault. Ask the adapter what the strategy is worth, look at
/// what the idle account holds, and adopt both. A loss lowers the share price
/// at once. A gain is locked and reaches the price evenly over the vault's
/// unlock period, with the performance fee charged as it does. Nothing else
/// changes the share price without a deposit or a payout.
///
/// This is also how tokens sent straight to the vault get counted. Anyone
/// can send the asset to the idle account, as an incentive for holders, say;
/// it is a gain like any other and unlocks the same way, so nobody can
/// deposit just ahead of it and walk away with it.
///
/// Anyone can call it. Deposits and fulfilments need a recent one, so a
/// client puts this instruction ahead of them in the same transaction.
///
/// > Call the adapter's `simulate` for the value it claims
/// > Store it and the idle balance, lock any gain, take any loss
///
/// Accounts:
///
/// 1. vault:               [mut]
/// 2. idle_account:
/// 3. event_authority:
/// 4. program:             [executable]    this program, for the event CPI
/// 5. ..                                   see `Adapter`
///
/// Parameters:
/// 1. data: [u8],                  // opaque, forwarded to the adapter
///
/// Account Checks:
/// - Vault: writable, deserialized
/// - IdleAccount: equal to the vault's
/// - Adapter accounts: checked by `Adapter::load`
///
/// Return Data:
/// - value: u64,                   // the debt now stored
///
/// Event Data:
/// - discriminator: u8, (255u8, 40u8)
/// - vault: Pubkey,
/// - claimed: u64,
/// - value: u64,
/// - idle: u64,
pub struct Simulate<'a> {
    pub vault: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub adapter: Adapter<'a>,
    pub data: &'a [u8],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Simulate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Simulate");

        let [vault, idle_account, event_authority, program, adapter @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let vault_data = Vault::load(vault)?;
        if vault_data.idle_account().ne(idle_account.key()) {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        let adapter = Adapter::load(adapter, vault_data)?;

        // Return the initialized struct
        Ok(Self {
            vault,
            idle_account,
            event_authority,
            program,
            adapter,
            data,
        })
    }
}

impl<'a> Simulate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &40;

    pub fn process(&mut self) -> ProgramResult {
        let claimed = self.adapter.simulate(self.data)?;

        let (slot, now) = clock()?;
        let vault = Vault::load_mut(self.vault)?;
        let held = TokenAccount::load(self.idle_account, vault.asset_token_program())?.amount;
        let value = vault.report(claimed, held, now, slot)?;
        let idle = vault.idle();

        // Log the Simulate Event
        emit(
            self.event_authority,
            self.program,
            Self::DISCRIMINATOR,
            &[
                self.vault.key(),
                &claimed.to_le_bytes(),
                &value.to_le_bytes(),
                &idle.to_le_bytes(),
            ],
        )?;

        // Last: any CPI after this would clear it
        set_return_data(&value.to_le_bytes());
        Ok(())
    }
}
