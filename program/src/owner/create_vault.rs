use crate::constants::{
    CONFIG_LEN, STRATEGY_SEED, VAULT_ACTIVE, VAULT_LEN, VAULT_SEED, VAULT_VERSION,
};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{
    check_mint, check_share_mint, check_uninitialized, clock, create_pda, TokenAccount,
};
use crate::state::{Config, Load, Vault};
use core::mem::size_of;
use pinocchio::log::sol_log;
use pinocchio::pubkey::{create_program_address, Pubkey};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # CreateVault
///
/// Create a vault: one asset in, one share token out, one adapter behind it.
/// The adapter is the only program deployed funds are ever handed to, and it
/// is fixed here for good. A vault whose funds are managed off chain uses an
/// adapter for that too.
///
/// The client creates the share mint and the token accounts first; this
/// instruction only accepts them if the vault, and nothing else, controls them.
/// Do it all in one transaction: the vault's address depends on the share mint
/// alone, so anyone who sees the mint could otherwise create the vault first.
///
/// > Create the Vault account at its seed-derived PDA
/// > Record the asset, the share mint, the token accounts and the adapter
/// > Apply the initial configuration; later ones wait out the timelock
///
/// Accounts:
///
/// 1. payer:               [signer, mut]   pays rent
/// 2. owner:               [signer]
/// 3. vault:               [mut]           PDA [VAULT_SEED, share_mint]
/// 4. asset_mint:
/// 5. share_mint:
/// 6. idle_account:                        asset token account owned by the vault
/// 7. escrow_account:                      share token account owned by the vault
/// 8. strategy_account:                    asset token account owned by the strategy authority
/// 9. system_program:      [executable]
/// 10. event_authority:
/// 11. program:            [executable]    this program, for the event CPI
///
/// Parameters:
/// 1. bump: u8,                    // vault PDA bump
/// 2. strategy_bump: u8,           // strategy authority PDA bump
/// 3. adapter: Pubkey,
/// 4. timelock: u64,               // seconds a later configuration waits
/// 5. config: Config,
///
/// Account Checks:
/// - Payer, Owner: signers
/// - Vault: writable, empty system account, at its PDA
/// - AssetMint: SPL Token, or Token-2022 with no transfer hook program set
/// - ShareMint: the same, with the vault as mint authority
/// - IdleAccount, EscrowAccount, StrategyAccount: the expected mint and owner, no delegate, no close authority
///
/// Instruction Checks:
/// - Adapter: set
/// - Config: fees and fulfil delay at or below their ceilings
///
/// Event Data:
/// - discriminator: u8, (255u8, 0u8)
/// - vault: Pubkey,
/// - owner: Pubkey,
/// - asset_mint: Pubkey,
/// - adapter: Pubkey,
pub struct CreateVaultAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub owner: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub escrow_account: &'a AccountInfo,
    pub strategy_account: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for CreateVaultAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, owner, vault, asset_mint, share_mint, idle_account, escrow_account, strategy_account, _system_program, event_authority, program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !payer.is_signer() || !owner.is_signer() {
            return Err(VaultError::NotSigner.into());
        }
        if !vault.is_writable() {
            return Err(VaultError::NotMutable.into());
        }
        check_uninitialized(vault)?;

        // Return the accounts
        Ok(Self {
            payer,
            owner,
            vault,
            asset_mint,
            share_mint,
            idle_account,
            escrow_account,
            strategy_account,
            event_authority,
            program,
        })
    }
}

pub struct CreateVaultInstructionData<'a> {
    pub bump: u8,
    pub strategy_bump: u8,
    pub adapter: &'a Pubkey,
    pub timelock: u64,
    pub config: &'a Config,
}

impl<'a> TryFrom<&'a [u8]> for CreateVaultInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data
            .len()
            .ne(&(2 * size_of::<u8>() + size_of::<Pubkey>() + size_of::<u64>() + CONFIG_LEN))
        {
            return Err(ProgramError::InvalidInstructionData);
        }

        let bump = data[0];
        let strategy_bump = data[1];
        let adapter: &Pubkey = data[2..34].try_into().unwrap();
        let timelock = u64::from_le_bytes(data[34..42].try_into().unwrap());

        // Instruction Checks
        if adapter.eq(&[0; 32]) {
            return Err(VaultError::InvalidAdapter.into());
        }
        let config = Config::from_bytes(&data[42..])?;

        Ok(Self {
            bump,
            strategy_bump,
            adapter,
            timelock,
            config,
        })
    }
}

pub struct CreateVault<'a> {
    pub accounts: CreateVaultAccounts<'a>,
    pub instruction_data: CreateVaultInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for CreateVault<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("CreateVault");

        let accounts = CreateVaultAccounts::try_from(accounts)?;
        let instruction_data = CreateVaultInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> CreateVault<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let vault_key = self.accounts.vault.key();
        let share_mint = self.accounts.share_mint.key();
        let asset_mint = self.accounts.asset_mint.key();

        // The vault must be the PDA for (share_mint, bump): one vault per share mint
        let expected = create_program_address(
            &[VAULT_SEED, share_mint, &[self.instruction_data.bump]],
            &crate::ID,
        )
        .map_err(|_| VaultError::InvalidSeeds)?;
        if expected.ne(vault_key) {
            return Err(VaultError::InvalidSeeds.into());
        }

        // Two different mints: neither runs a hook on transfer, and the vault
        // issues the shares
        if asset_mint.eq(share_mint) {
            return Err(VaultError::InvalidMint.into());
        }
        let decimals = check_mint(self.accounts.asset_mint)?;
        check_share_mint(self.accounts.share_mint, vault_key)?;

        // The vault alone controls idle funds and escrowed shares
        // SAFETY: an account's owner is not mutated during this instruction.
        let asset_token_program = unsafe { self.accounts.asset_mint.owner() };
        TokenAccount::load(self.accounts.idle_account, asset_token_program)?
            .check_clean(asset_mint, vault_key)?;
        // SAFETY: as above.
        let share_token_program = unsafe { self.accounts.share_mint.owner() };
        TokenAccount::load(self.accounts.escrow_account, share_token_program)?
            .check_clean(share_mint, vault_key)?;

        // The adapter gets its own authority, which controls the strategy account only
        let strategy_authority = create_program_address(
            &[
                STRATEGY_SEED,
                vault_key,
                &[self.instruction_data.strategy_bump],
            ],
            &crate::ID,
        )
        .map_err(|_| VaultError::InvalidSeeds)?;
        TokenAccount::load(self.accounts.strategy_account, asset_token_program)?
            .check_clean(asset_mint, &strategy_authority)?;

        // Create the Vault account
        let seeds = Vault::seeds(share_mint, self.instruction_data.bump);
        create_pda(
            self.accounts.payer,
            self.accounts.vault,
            VAULT_LEN,
            &seeds.as_seeds(),
        )?;

        // Populate it; balances, the pending owner and the pending config stay zero
        let (slot, now) = clock()?;
        let vault = Vault::load_new(self.accounts.vault)?;
        vault.set_version(VAULT_VERSION);
        vault.set_status(VAULT_ACTIVE);
        vault.set_bump(self.instruction_data.bump);
        vault.set_strategy_bump(self.instruction_data.strategy_bump);
        vault.set_decimals(decimals);
        vault.set_owner(*self.accounts.owner.key());
        vault.set_asset_mint(*asset_mint);
        vault.set_asset_token_program(*asset_token_program);
        vault.set_share_mint(*share_mint);
        vault.set_idle_account(*self.accounts.idle_account.key());
        vault.set_escrow_account(*self.accounts.escrow_account.key());
        vault.set_adapter(*self.instruction_data.adapter);
        vault.set_strategy_authority(strategy_authority);
        vault.set_strategy_account(*self.accounts.strategy_account.key());
        vault.set_last_report_slot(slot);
        vault.set_unlock_ts(now);
        vault.set_unlock_end(now);
        vault.set_fee_ts(now);
        vault.set_timelock(self.instruction_data.timelock);
        vault.config = *self.instruction_data.config;

        // Log the CreateVault Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[
                vault_key,
                self.accounts.owner.key(),
                asset_mint,
                self.instruction_data.adapter,
            ],
        )
    }
}
