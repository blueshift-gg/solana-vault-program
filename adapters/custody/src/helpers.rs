//! Views of other programs' accounts, and the one transfer this program makes.
//! Checks live in `check_*`/`load` helpers; the rest only executes.

use crate::errors::CustodyError;
use pinocchio::{
    account_info::AccountInfo,
    cpi::invoke_signed,
    instruction::{AccountMeta, Instruction, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};
use vault_core::constants::*;
use vault_core::state::Vault;

/// The current Unix timestamp.
#[inline(always)]
pub fn now() -> Result<i64, ProgramError> {
    Ok(Clock::get()?.unix_timestamp)
}

/// View a vault account of the vault program that uses this adapter.
#[inline(always)]
pub fn load_vault(account: &AccountInfo) -> Result<&Vault, ProgramError> {
    if !account.is_owned_by(&vault_core::ID) || account.data_len() != VAULT_LEN {
        return Err(CustodyError::InvalidVault.into());
    }
    // SAFETY: length checked above; all fields have alignment 1.
    let vault = unsafe { Vault::from_bytes_unchecked(account.borrow_data_unchecked()) };
    if vault.version() != VAULT_VERSION || vault.adapter().ne(&crate::ID) {
        return Err(CustodyError::InvalidVault.into());
    }
    Ok(vault)
}

/// The fields of an SPL token account this program reads.
pub struct TokenAccount<'a> {
    pub mint: &'a Pubkey,
    pub owner: &'a Pubkey,
    pub amount: u64,
    data: &'a [u8],
}

impl<'a> TokenAccount<'a> {
    /// Validate owner program and length, then view the fields in place.
    #[inline(always)]
    pub fn load(account: &'a AccountInfo, token_program: &Pubkey) -> Result<Self, ProgramError> {
        // SAFETY: the program holds no mutable borrow of a token account.
        let data = unsafe { account.borrow_data_unchecked() };
        if !account.is_owned_by(token_program) || data.len() < TOKEN_ACCOUNT_LEN {
            return Err(CustodyError::InvalidTokenAccount.into());
        }
        // SAFETY: the length check above covers every offset read; `Pubkey`
        // has alignment 1.
        Ok(Self {
            mint: unsafe { &*(data.as_ptr() as *const Pubkey) },
            owner: unsafe { &*(data.as_ptr().add(TOKEN_ACCOUNT_OWNER) as *const Pubkey) },
            amount: u64::from_le_bytes(
                data[TOKEN_ACCOUNT_AMOUNT..TOKEN_ACCOUNT_AMOUNT + 8]
                    .try_into()
                    .unwrap(),
            ),
            data,
        })
    }

    /// Initialized and not frozen.
    #[inline(always)]
    pub fn is_usable(&self) -> bool {
        self.data[TOKEN_ACCOUNT_STATE] == 1
    }

    /// No delegate and no close authority: nobody but its owner has, or can
    /// gain, a way in. A balance is fine; anyone can send tokens to an account.
    #[inline(always)]
    pub fn is_clean(&self) -> bool {
        self.data[TOKEN_ACCOUNT_DELEGATE..TOKEN_ACCOUNT_DELEGATE + 4] == [0; 4]
            && self.data[TOKEN_ACCOUNT_CLOSE_AUTHORITY..TOKEN_ACCOUNT_CLOSE_AUTHORITY + 4] == [0; 4]
    }
}

/// `TransferChecked`, which SPL Token and Token-2022 share. The decimals are
/// read from the mint.
#[inline(always)]
pub fn transfer(
    token_program: &AccountInfo,
    from: &AccountInfo,
    mint: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    signers: &[Signer],
) -> ProgramResult {
    let mut data = [0u8; 10];
    data[0] = 12;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    // SAFETY: the program holds no mutable borrow of a mint.
    data[9] = unsafe { mint.borrow_data_unchecked() }
        .get(44)
        .copied()
        .ok_or(ProgramError::InvalidAccountData)?;
    invoke_signed(
        &Instruction {
            program_id: token_program.key(),
            accounts: &[
                AccountMeta::writable(from.key()),
                AccountMeta::readonly(mint.key()),
                AccountMeta::writable(to.key()),
                AccountMeta::readonly_signer(authority.key()),
            ],
            data: &data,
        },
        &[from, mint, to, authority],
        signers,
    )
}
