use crate::constants::{PERMIT_DEPOSIT, PERMIT_LEN, VAULT_ACTIVE};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{check_permit, clock, mint_shares, receive};
use crate::state::{Load, Vault};
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Deposit
///
/// Put assets into the vault and receive shares for them, at the price right
/// now. The price must be fresh: when the vault has debt, a client puts
/// `Simulate` ahead of this instruction in the same transaction. The deposit
/// itself never calls the adapter.
///
/// If the vault has a deposit authority, the depositor also presents that
/// authority's permit, which is how a vault admits only whom it has cleared.
///
/// > Transfer the assets into the idle account
/// > Price what arrived from `idle + debt` and total shares
/// > Mint the shares to the depositor
///
/// Accounts:
///
/// 1. depositor:           [signer]
/// 2. vault:               [mut]
/// 3. asset_mint:
/// 4. share_mint:          [mut]
/// 5. idle_account:        [mut]
/// 6. depositor_assets:    [mut]
/// 7. depositor_shares:    [mut]
/// 8. asset_token_program: [executable]
/// 9. share_token_program: [executable]
/// 10. event_authority:
/// 11. program:            [executable]    this program, for the event CPI
///
/// Parameters:
/// 1. assets: u64,
/// 2. min_shares_out: u64,         // fewest shares the depositor accepts
/// 3. permit: [u8; 72],            // optional: expires_at, then the signature
///
/// Account Checks:
/// - Depositor: signer
/// - Vault: writable, deserialized, active
/// - AssetMint, ShareMint, IdleAccount: equal to the vault's
/// - DepositorAssets, DepositorShares: no need to check since the transfer and the mint fail on a wrong mint or owner
///
/// Instruction Checks:
/// - Assets: nonzero
/// - Permit, freshness, deposit cap and slippage: need the vault, so they run in process
///
/// Event Data:
/// - discriminator: u8, (255u8, 30u8)
/// - vault: Pubkey,
/// - depositor: Pubkey,
/// - assets: u64,                  // what arrived in idle
/// - shares: u64,
pub struct DepositAccounts<'a> {
    pub depositor: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub depositor_assets: &'a AccountInfo,
    pub depositor_shares: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for DepositAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [depositor, vault, asset_mint, share_mint, idle_account, depositor_assets, depositor_shares, _asset_token_program, _share_token_program, event_authority, program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !depositor.is_signer() {
            return Err(VaultError::NotSigner.into());
        }
        let vault_data = Vault::load(vault)?;
        if vault_data.status() != VAULT_ACTIVE {
            return Err(VaultError::InvalidStatus.into());
        }
        if vault_data.asset_mint().ne(asset_mint.key())
            || vault_data.share_mint().ne(share_mint.key())
            || vault_data.idle_account().ne(idle_account.key())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            depositor,
            vault,
            asset_mint,
            share_mint,
            idle_account,
            depositor_assets,
            depositor_shares,
            event_authority,
            program,
        })
    }
}

pub struct DepositInstructionData<'a> {
    pub assets: u64,
    pub min_shares_out: u64,
    pub permit: Option<&'a [u8; PERMIT_LEN]>,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let Some((amounts, permit)) = data.split_first_chunk::<{ 2 * size_of::<u64>() }>() else {
            return Err(ProgramError::InvalidInstructionData);
        };
        let permit = match permit.len() {
            0 => None,
            PERMIT_LEN => Some(permit.try_into().unwrap()),
            _ => return Err(ProgramError::InvalidInstructionData),
        };

        let assets = u64::from_le_bytes(amounts[0..8].try_into().unwrap());
        let min_shares_out = u64::from_le_bytes(amounts[8..16].try_into().unwrap());

        // Instruction Checks
        if assets == 0 {
            return Err(VaultError::ZeroAmount.into());
        }

        Ok(Self {
            assets,
            min_shares_out,
            permit,
        })
    }
}

pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub instruction_data: DepositInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Deposit");

        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a u8 = &30;

    pub fn process(&mut self) -> ProgramResult {
        let assets = self.instruction_data.assets;
        let (slot, now) = clock()?;

        // Only whom the deposit authority permitted, if the vault has one
        let vault = Vault::load_mut(self.accounts.vault)?;
        if vault.config.deposit_authority().ne(&[0; 32]) {
            check_permit(
                vault.config.deposit_authority(),
                PERMIT_DEPOSIT,
                self.accounts.vault.key(),
                self.accounts.depositor.key(),
                0,
                self.instruction_data.permit,
                now,
            )?;
        }

        // Only a fresh price, and only up to the deposit cap
        vault.check_fresh(slot)?;
        if assets > vault.max_deposit()? {
            return Err(VaultError::AmountTooLarge.into());
        }

        // Take the assets and count what arrives
        let received = receive(
            vault.asset_token_program(),
            self.accounts.depositor_assets,
            self.accounts.asset_mint,
            self.accounts.idle_account,
            self.accounts.depositor,
            assets,
            vault.decimals(),
            &[],
        )?;

        // Price what arrived at the price before it did
        let shares = vault.deposit(received, now)?;
        if shares == 0 {
            return Err(VaultError::ZeroAmount.into());
        }
        if shares < self.instruction_data.min_shares_out {
            return Err(VaultError::SlippageExceeded.into());
        }

        // Mint the shares, signed by the vault
        let seeds = vault.signer_seeds();
        let seeds = seeds.as_seeds();
        mint_shares(
            self.accounts.share_mint,
            self.accounts.depositor_shares,
            self.accounts.vault,
            shares,
            &[Signer::from(&seeds)],
        )?;

        // Log the Deposit Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[
                self.accounts.vault.key(),
                self.accounts.depositor.key(),
                &received.to_le_bytes(),
                &shares.to_le_bytes(),
            ],
        )
    }
}
