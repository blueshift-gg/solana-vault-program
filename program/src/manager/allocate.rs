use crate::adapter::Adapter;
use crate::constants::VAULT_ACTIVE;
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{check_role, receive};
use crate::state::{Load, Vault};
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Allocate
///
/// Put idle funds to work: move them to the strategy account and tell the
/// adapter to take them. What moves becomes debt. It is principal, not a
/// gain, so the share price does not change.
///
/// > Transfer to the strategy account, signed by the vault
/// > Call the adapter's `deposit`, signed by the strategy authority
/// > Sweep back whatever the adapter did not take
///
/// Accounts:
///
/// 1. manager:             [signer]
/// 2. vault:               [mut]
/// 3. idle_account:        [mut]
/// 4. event_authority:
/// 5. program:             [executable]    this program, for the event CPI
/// 6. ..                                   see `Adapter`
///
/// Parameters:
/// 1. amount: u64,
/// 2. data: [u8],                  // opaque, forwarded to the adapter
///
/// Account Checks:
/// - Manager: signer, the vault's manager
/// - Vault: writable, deserialized, active
/// - IdleAccount: equal to the vault's
/// - Adapter accounts: checked by `Adapter::load`
///
/// Instruction Checks:
/// - Amount: nonzero, at most idle, and debt stays at or below the debt cap
///
/// Event Data:
/// - discriminator: u8, (255u8, 20u8)
/// - vault: Pubkey,
/// - amount: u64,
pub struct AllocateAccounts<'a> {
    pub manager: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub adapter: Adapter<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for AllocateAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [manager, vault, idle_account, event_authority, program, adapter @ ..] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let vault_data = Vault::load(vault)?;
        check_role(manager, vault_data.config.manager())?;
        if vault_data.status() != VAULT_ACTIVE {
            return Err(VaultError::InvalidStatus.into());
        }
        if vault_data.idle_account().ne(idle_account.key()) {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        let adapter = Adapter::load(adapter, vault_data)?;

        // Return the accounts
        Ok(Self {
            manager,
            vault,
            idle_account,
            event_authority,
            program,
            adapter,
        })
    }
}

pub struct AllocateInstructionData<'a> {
    pub amount: u64,
    pub data: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for AllocateInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() < size_of::<u64>() {
            return Err(ProgramError::InvalidInstructionData);
        }
        let (amount, data) = data.split_at(size_of::<u64>());
        let amount = u64::from_le_bytes(amount.try_into().unwrap());

        // Instruction Checks
        if amount == 0 {
            return Err(VaultError::ZeroAmount.into());
        }

        Ok(Self { amount, data })
    }
}

pub struct Allocate<'a> {
    pub accounts: AllocateAccounts<'a>,
    pub instruction_data: AllocateInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Allocate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Allocate");

        let accounts = AllocateAccounts::try_from(accounts)?;
        let instruction_data = AllocateInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Allocate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &20;

    pub fn process(&mut self) -> ProgramResult {
        let amount = self.instruction_data.amount;
        let adapter = &self.accounts.adapter;

        // Send the principal to the strategy account, signed by the vault,
        // and count what arrives
        let vault = Vault::load_mut(self.accounts.vault)?;
        if amount > vault.idle() {
            return Err(VaultError::AmountTooLarge.into());
        }
        let vault_seeds = vault.signer_seeds();
        let vault_seeds = vault_seeds.as_seeds();
        let received = receive(
            vault.asset_token_program(),
            self.accounts.idle_account,
            adapter.mint,
            adapter.account,
            self.accounts.vault,
            amount,
            vault.decimals(),
            &[Signer::from(&vault_seeds)],
        )?;

        // Move it from idle to debt, within the debt cap
        vault.deploy(amount, received)?;
        if vault.debt() > vault.config.debt_cap() {
            return Err(VaultError::AmountTooLarge.into());
        }

        // Hand it to the adapter, then take back whatever it did not use
        let seeds = Vault::strategy_seeds(self.accounts.vault.key(), vault.strategy_bump());
        let seeds = seeds.as_seeds();
        let signer = Signer::from(&seeds);
        adapter.deposit(received, self.instruction_data.data, &signer)?;
        adapter.sweep(self.accounts.vault, self.accounts.idle_account, &signer)?;

        // Log the Allocate Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), &amount.to_le_bytes()],
        )
    }
}
