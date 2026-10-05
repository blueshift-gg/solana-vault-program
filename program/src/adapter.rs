//! The one place the program calls code it does not control.
//!
//! An adapter receives the strategy authority as its only signer, and that
//! PDA owns nothing but the strategy token account. The vault PDA, which owns
//! idle funds and the share mint, never signs an adapter call. Nothing an
//! adapter returns is trusted: `simulate`'s value is a claim for
//! `Vault::report` to clamp, and principal is counted by `sweep`, which reads
//! what actually arrived in the strategy account.

use crate::{
    constants::*,
    errors::VaultError,
    helpers::{receive, TokenAccount},
    state::{Load, Vault},
};
use pinocchio::{
    account_info::AccountInfo,
    cpi::{get_return_data, slice_invoke_signed},
    instruction::{AccountMeta, Instruction, Signer},
    program_error::ProgramError,
    ProgramResult,
};

/// The trailing accounts of an instruction that reaches the adapter:
///
/// 1. strategy_authority:                 writable if the adapter's protocol requires it of a signer
/// 2. strategy_account:    [mut]
/// 3. asset_mint:
/// 4. token_program:       [executable]
/// 5. adapter:             [executable]
/// 6. ..                                   the adapter's own accounts, forwarded as given
pub struct Adapter<'a> {
    pub authority: &'a AccountInfo,
    pub account: &'a AccountInfo,
    pub mint: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub rest: &'a [AccountInfo],
}

/// Accounts every adapter call starts with: the first four above.
const PREFIX: usize = 4;

impl<'a> Adapter<'a> {
    /// Check the accounts against what the vault recorded at creation.
    #[inline(always)]
    pub fn load(accounts: &'a [AccountInfo], vault: &Vault) -> Result<Self, ProgramError> {
        let [authority, account, mint, token_program, program, rest @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        if program.key().ne(vault.adapter())
            || authority.key().ne(vault.strategy_authority())
            || account.key().ne(vault.strategy_account())
            || mint.key().ne(vault.asset_mint())
            || token_program.key().ne(vault.asset_token_program())
        {
            return Err(VaultError::InvalidAdapter.into());
        }
        Ok(Self {
            authority,
            account,
            mint,
            token_program,
            program,
            rest,
        })
    }

    /// The value claimed for the strategy: what the adapter reports for its
    /// position, plus what sits in the strategy account. Unsigned: valuing a
    /// position needs no authority over it.
    pub fn simulate(&self, data: &[u8]) -> Result<u64, ProgramError> {
        let held = self.balance()?;
        self.invoke(&ADAPTER_SIMULATE, None, data, &[])?;
        // Return data is cleared before each CPI, so what is here was set
        // during this call; the id tells the adapter from a program it called.
        match get_return_data() {
            Some(value) if value.program_id().eq(self.program.key()) && value.len() == 8 => {
                let position = u64::from_le_bytes(value.as_slice().try_into().unwrap());
                Ok(position.saturating_add(held))
            }
            _ => Err(VaultError::InvalidReturnData.into()),
        }
    }

    /// Ask the adapter to take `amount` out of the strategy account.
    pub fn deposit(&self, amount: u64, data: &[u8], signer: &Signer) -> ProgramResult {
        self.invoke(
            &ADAPTER_DEPOSIT,
            Some(amount),
            data,
            core::slice::from_ref(signer),
        )
    }

    /// Ask the adapter to put up to `amount` back into the strategy account.
    pub fn withdraw(&self, amount: u64, data: &[u8], signer: &Signer) -> ProgramResult {
        self.invoke(
            &ADAPTER_WITHDRAW,
            Some(amount),
            data,
            core::slice::from_ref(signer),
        )
    }

    /// What sits in the strategy account right now.
    pub fn balance(&self) -> Result<u64, ProgramError> {
        Ok(TokenAccount::load(self.account, self.token_program.key())?.amount)
    }

    /// Move what sits in the strategy account back to idle as principal, up
    /// to `debt`. Every adapter call that moves funds ends here. Anything
    /// beyond `debt`, a gain not reported yet or a donation, stays in the
    /// strategy account, where `Simulate` counts it as a gain and locks it.
    pub fn sweep(
        &self,
        vault_account: &AccountInfo,
        idle_account: &AccountInfo,
        signer: &Signer,
    ) -> Result<u64, ProgramError> {
        let vault = Vault::load_mut(vault_account)?;
        let amount = self.balance()?.min(vault.debt());
        if amount == 0 {
            return Ok(0);
        }
        let received = receive(
            self.token_program.key(),
            self.account,
            self.mint,
            idle_account,
            self.authority,
            amount,
            vault.decimals(),
            core::slice::from_ref(signer),
        )?;
        vault.recall(amount, received)?;
        Ok(received)
    }

    /// `[discriminator][amount][data]` to the adapter, with the prefix
    /// accounts first and the adapter's own after.
    #[inline(never)]
    fn invoke(
        &self,
        discriminator: &[u8; 8],
        amount: Option<u64>,
        data: &[u8],
        signers: &[Signer],
    ) -> ProgramResult {
        let count = PREFIX + self.rest.len();
        if count > MAX_ADAPTER_ACCOUNTS || data.len() > MAX_ADAPTER_DATA {
            return Err(VaultError::AdapterCallTooLarge.into());
        }

        // Slots past `count` are never passed on; they repeat the mint.
        let metas: [AccountMeta; MAX_ADAPTER_ACCOUNTS] = core::array::from_fn(|i| match i {
            // Writable if the caller made it so: some protocols require it of a signer.
            0 => AccountMeta::new(
                self.authority.key(),
                self.authority.is_writable(),
                !signers.is_empty(),
            ),
            1 => AccountMeta::writable(self.account.key()),
            3 => AccountMeta::readonly(self.token_program.key()),
            _ => match i.checked_sub(PREFIX).and_then(|i| self.rest.get(i)) {
                // Never as a signer: the strategy authority is all an adapter gets.
                Some(account) => AccountMeta::new(account.key(), account.is_writable(), false),
                None => AccountMeta::readonly(self.mint.key()),
            },
        });
        let infos: [&AccountInfo; MAX_ADAPTER_ACCOUNTS] = core::array::from_fn(|i| match i {
            0 => self.authority,
            1 => self.account,
            3 => self.token_program,
            _ => i
                .checked_sub(PREFIX)
                .and_then(|i| self.rest.get(i))
                .unwrap_or(self.mint),
        });

        let mut buffer = [0u8; 16 + MAX_ADAPTER_DATA];
        buffer[..8].copy_from_slice(discriminator);
        let mut len = 8;
        if let Some(amount) = amount {
            buffer[8..16].copy_from_slice(&amount.to_le_bytes());
            len = 16;
        }
        buffer[len..len + data.len()].copy_from_slice(data);
        len += data.len();

        cpi(
            &Instruction {
                program_id: self.program.key(),
                accounts: &metas[..count],
                data: &buffer[..len],
            },
            &infos[..count],
            signers,
        )
    }
}

/// `slice_invoke_signed` keeps a 64-account scratch array on its stack. Its
/// own frame keeps that out of `invoke`'s, which would otherwise exceed the
/// 4 KiB an SBF frame allows.
#[inline(never)]
fn cpi(instruction: &Instruction, accounts: &[&AccountInfo], signers: &[Signer]) -> ProgramResult {
    slice_invoke_signed(instruction, accounts, signers)
}
