use crate::adapter::Adapter;
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::check_role;
use crate::state::{Load, Vault};
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Deallocate
///
/// Bring funds back from the strategy to idle, to rebalance or to make
/// redemptions payable. The adapter is asked for an amount; what actually
/// lands in the strategy account is what gets counted, as principal and up to
/// the debt. A gain is never credited here: that is `Simulate`'s job. Works in
/// every vault status, so funds can always come home.
///
/// > Call the adapter's `withdraw`, signed by the strategy authority
/// > Sweep the strategy account into idle
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
/// - Vault: writable, deserialized
/// - IdleAccount: equal to the vault's
/// - Adapter accounts: checked by `Adapter::load`
///
/// Instruction Checks:
/// - Amount: nonzero
///
/// Event Data:
/// - discriminator: u8, (255u8, 21u8)
/// - vault: Pubkey,
/// - returned: u64,
pub struct DeallocateAccounts<'a> {
    pub manager: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub adapter: Adapter<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for DeallocateAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [manager, vault, idle_account, event_authority, program, adapter @ ..] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let vault_data = Vault::load(vault)?;
        check_role(manager, vault_data.config.manager())?;
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

pub struct DeallocateInstructionData<'a> {
    pub amount: u64,
    pub data: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for DeallocateInstructionData<'a> {
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

pub struct Deallocate<'a> {
    pub accounts: DeallocateAccounts<'a>,
    pub instruction_data: DeallocateInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deallocate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Deallocate");

        let accounts = DeallocateAccounts::try_from(accounts)?;
        let instruction_data = DeallocateInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Deallocate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &21;

    pub fn process(&mut self) -> ProgramResult {
        let adapter = &self.accounts.adapter;

        // The adapter refills the strategy account; the sweep counts it
        let strategy_bump = Vault::load(self.accounts.vault)?.strategy_bump();
        let seeds = Vault::strategy_seeds(self.accounts.vault.key(), strategy_bump);
        let seeds = seeds.as_seeds();
        let signer = Signer::from(&seeds);
        adapter.withdraw(
            self.instruction_data.amount,
            self.instruction_data.data,
            &signer,
        )?;
        let returned = adapter.sweep(self.accounts.vault, self.accounts.idle_account, &signer)?;

        // Log the Deallocate Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[self.accounts.vault.key(), &returned.to_le_bytes()],
        )
    }
}
