//! Account lifecycle, role checks, SPL token views and transfers.
//! Checks live in `check_*`/`load` helpers; the rest only executes.

use crate::{constants::*, errors::VaultError};
use pinocchio::{
    account_info::AccountInfo,
    cpi::invoke_signed,
    instruction::{AccountMeta, Instruction, Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};

/// The current `(slot, unix_timestamp)`.
#[inline(always)]
pub fn clock() -> Result<(u64, i64), ProgramError> {
    let clock = Clock::get()?;
    Ok((clock.slot, clock.unix_timestamp))
}

/// `signer` signed the transaction and holds `role`. An unset role is all
/// zeros, which no one can sign for.
#[inline(always)]
pub fn check_role(signer: &AccountInfo, role: &Pubkey) -> ProgramResult {
    if !signer.is_signer() {
        return Err(VaultError::NotSigner.into());
    }
    if signer.key().ne(role) {
        return Err(VaultError::InvalidAuthority.into());
    }
    Ok(())
}

/// `authority` permitted `subject`, with `nonce`, to do `kind` in `vault`,
/// and the permit has not expired. `permit` is `[expires_at: i64][signature: [u8; 64]]`: a
/// signature made off chain, so the authority never signs a transaction and
/// anyone holding the permit can present it.
pub fn check_permit(
    authority: &Pubkey,
    kind: u8,
    vault: &Pubkey,
    subject: &Pubkey,
    nonce: u64,
    permit: Option<&[u8; PERMIT_LEN]>,
    now: i64,
) -> ProgramResult {
    let Some(permit) = permit else {
        return Err(VaultError::InvalidPermit.into());
    };
    let expires_at = i64::from_le_bytes(permit[..8].try_into().unwrap());
    if now > expires_at {
        return Err(VaultError::PermitExpired.into());
    }
    let signature: &[u8; 64] = permit[8..].try_into().unwrap();
    brine_ed25519::verify_strict(
        &brine_ed25519::Address::new_from_array(*authority),
        signature,
        &[&vault_core::permit_message(
            kind, vault, subject, nonce, expires_at,
        )],
    )
    .map_err(|_| VaultError::InvalidPermit.into())
}

/// An account about to be created must be an empty system account.
#[inline(always)]
pub fn check_uninitialized(account: &AccountInfo) -> ProgramResult {
    if !account.is_owned_by(&pinocchio_system::ID) || account.data_len() != 0 {
        return Err(VaultError::AlreadyInitialized.into());
    }
    Ok(())
}

/// Create a program-owned PDA, rent paid by `payer`. `bump` is the last seed.
#[inline(always)]
pub fn create_pda(
    payer: &AccountInfo,
    account: &AccountInfo,
    space: usize,
    seeds: &[Seed],
) -> ProgramResult {
    let lamports = Rent::get()?.minimum_balance(space);
    if account.lamports() == 0 {
        return pinocchio_system::instructions::CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: space as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&[Signer::from(seeds)]);
    }
    // Anyone can send lamports to a PDA before initialization.
    let missing = lamports.saturating_sub(account.lamports());
    if missing > 0 {
        pinocchio_system::instructions::Transfer {
            from: payer,
            to: account,
            lamports: missing,
        }
        .invoke()?;
    }
    pinocchio_system::instructions::Allocate {
        account,
        space: space as u64,
    }
    .invoke_signed(&[Signer::from(seeds)])?;
    pinocchio_system::instructions::Assign {
        account,
        owner: &crate::ID,
    }
    .invoke_signed(&[Signer::from(seeds)])
}

/// Move every lamport out and close a program-owned account.
#[inline(always)]
pub fn close(account: &AccountInfo, to: &AccountInfo) -> ProgramResult {
    if account.key().eq(to.key()) {
        return Err(ProgramError::InvalidArgument);
    }
    // SAFETY: the program never holds a checked borrow on lamports, and `to`
    // is distinct from `account`, checked above.
    unsafe {
        *to.borrow_mut_lamports_unchecked() = to
            .lamports()
            .checked_add(account.lamports())
            .ok_or(ProgramError::ArithmeticOverflow)?;
    }
    account.close()
}

/// The fields of an SPL token account this program reads.
pub struct TokenAccount<'a> {
    pub mint: &'a Pubkey,
    pub owner: &'a Pubkey,
    pub amount: u64,
    data: &'a [u8],
}

impl<'a> TokenAccount<'a> {
    /// Validate owner program and layout, then view the fields in place. A
    /// Token-2022 account may carry extensions after the base layout; its
    /// type byte then tells it apart from a mint.
    #[inline(always)]
    pub fn load(account: &'a AccountInfo, token_program: &Pubkey) -> Result<Self, ProgramError> {
        // SAFETY: the program holds no mutable borrow of a token account.
        let data = unsafe { account.borrow_data_unchecked() };
        let layout = data.len() == 165 || (data.len() > 165 && data[165] == 2);
        if !account.is_owned_by(token_program) || !layout {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        // SAFETY: the length check above covers every offset read; `Pubkey`
        // has alignment 1.
        Ok(Self {
            mint: unsafe { &*(data.as_ptr() as *const Pubkey) },
            owner: unsafe { &*(data.as_ptr().add(32) as *const Pubkey) },
            amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            data,
        })
    }

    /// A token account the vault can rely on: the expected mint and owner,
    /// initialized and not frozen, with no delegate and no close authority,
    /// and not requiring a memo on incoming transfers, which the program's
    /// own transfers do not carry. Handing an account over with
    /// `SetAuthority` keeps all of these, so ownership alone is not enough.
    #[inline(always)]
    pub fn check_clean(&self, mint: &Pubkey, owner: &Pubkey) -> ProgramResult {
        let no_delegate = self.data[72..76] == [0; 4];
        let initialized = self.data[108] == 1;
        let no_close_authority = self.data[129..133] == [0; 4];
        let memo = self
            .data
            .get(166..)
            .map(|tlv| extension(tlv, EXTENSION_MEMO_TRANSFER));
        let no_memo = matches!(memo, None | Some(Ok(None)) | Some(Ok(Some([0]))));
        if self.mint.ne(mint)
            || self.owner.ne(owner)
            || !no_delegate
            || !initialized
            || !no_close_authority
            || !no_memo
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        Ok(())
    }
}

/// `TransferChecked` under either token program. SPL Token and Token-2022
/// share the instruction, so the asset's own program is the only difference.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn transfer(
    token_program: &Pubkey,
    from: &AccountInfo,
    mint: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    let mut data = [0u8; 10];
    data[0] = 12;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = decimals;
    invoke_signed(
        &Instruction {
            program_id: token_program,
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

/// `transfer`, returning what arrived in `to`. A Token-2022 mint can withhold
/// a fee from every transfer, so an amount coming in is counted, never assumed.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn receive(
    token_program: &Pubkey,
    from: &AccountInfo,
    mint: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> Result<u64, ProgramError> {
    let before = TokenAccount::load(to, token_program)?.amount;
    transfer(
        token_program,
        from,
        mint,
        to,
        authority,
        amount,
        decimals,
        signers,
    )?;
    TokenAccount::load(to, token_program)?
        .amount
        .checked_sub(before)
        .ok_or(VaultError::MathOverflow.into())
}

/// The decimals of a mint the vault can use, as the asset or for its shares:
/// an SPL Token mint, or a Token-2022 mint with no transfer hook program set.
/// A hook is the one thing ruled out: it runs foreign code inside every
/// transfer and needs accounts the vault does not carry. Everything else a
/// mint can do is for whoever chose it to weigh.
pub fn check_mint(mint: &AccountInfo) -> Result<u8, ProgramError> {
    // SAFETY: the program holds no mutable borrow of a mint.
    let data = unsafe { mint.borrow_data_unchecked() };
    let valid = if mint.is_owned_by(&pinocchio_token::ID) {
        data.len() == 82
    } else if mint.is_owned_by(&TOKEN_2022) {
        data.len() == 82 || (data.len() > 166 && data[165] == 1 && !has_transfer_hook(&data[166..]))
    } else {
        false
    };
    if !valid || data[45] != 1 {
        return Err(VaultError::InvalidMint.into());
    }
    Ok(data[44])
}

/// Find `wanted` in a Token-2022 TLV region: `[type: u16][length: u16][value]`
/// entries, ended by the region's end or an uninitialized (zero) type.
/// `Err` is a region that does not parse.
fn extension(mut tlv: &[u8], wanted: u16) -> Result<Option<&[u8]>, ()> {
    while tlv.len() >= 4 {
        let extension = u16::from_le_bytes([tlv[0], tlv[1]]);
        let length = u16::from_le_bytes([tlv[2], tlv[3]]) as usize;
        if extension == 0 {
            break;
        }
        let value = tlv.get(4..4 + length).ok_or(())?;
        if extension == wanted {
            return Ok(Some(value));
        }
        tlv = &tlv[4 + length..];
    }
    Ok(None)
}

/// Whether a mint's TLV region names a transfer hook program. The value is
/// `[authority: Pubkey][program: Pubkey]`; all zeros is no program. A region
/// that does not parse counts as having one.
fn has_transfer_hook(tlv: &[u8]) -> bool {
    match extension(tlv, EXTENSION_TRANSFER_HOOK) {
        Ok(Some(value)) => value.get(32..64) != Some(&[0; 32]),
        Ok(None) => false,
        Err(()) => true,
    }
}

/// A share mint the vault can issue from: a usable mint whose mint authority
/// is `authority`. What else the mint is, its decimals, its extensions, a
/// freeze authority, is for whoever created it to decide.
pub fn check_share_mint(mint: &AccountInfo, authority: &Pubkey) -> ProgramResult {
    check_mint(mint)?;
    // SAFETY: the program holds no mutable borrow of a mint.
    let data = unsafe { mint.borrow_data_unchecked() };
    if data[0..4] != [1, 0, 0, 0] || data[4..36] != authority[..] {
        return Err(VaultError::InvalidMint.into());
    }
    Ok(())
}

/// The decimals of a mint already checked at creation.
#[inline(always)]
pub fn decimals(mint: &AccountInfo) -> Result<u8, ProgramError> {
    // SAFETY: the program holds no mutable borrow of a mint.
    unsafe { mint.borrow_data_unchecked() }
        .get(44)
        .copied()
        .ok_or(VaultError::InvalidMint.into())
}

/// `MintTo` or `Burn` under either token program: `[instruction][amount]`
/// over `[mint or account, account or mint, authority]`, signed by the vault.
#[inline(always)]
fn mint_or_burn(
    instruction: u8,
    first: &AccountInfo,
    second: &AccountInfo,
    authority: &AccountInfo,
    token_program: &Pubkey,
    amount: u64,
    signers: &[Signer],
) -> ProgramResult {
    let mut data = [0u8; 9];
    data[0] = instruction;
    data[1..].copy_from_slice(&amount.to_le_bytes());
    invoke_signed(
        &Instruction {
            program_id: token_program,
            accounts: &[
                AccountMeta::writable(first.key()),
                AccountMeta::writable(second.key()),
                AccountMeta::readonly_signer(authority.key()),
            ],
            data: &data,
        },
        &[first, second, authority],
        signers,
    )
}

/// Mint `amount` shares to `account`. The share mint's own token program is
/// whichever owns it.
#[inline(always)]
pub fn mint_shares(
    mint: &AccountInfo,
    account: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    signers: &[Signer],
) -> ProgramResult {
    // SAFETY: an account's owner is not mutated during this instruction.
    let token_program = unsafe { mint.owner() };
    mint_or_burn(7, mint, account, authority, token_program, amount, signers)
}

/// Burn `amount` shares from `account`.
#[inline(always)]
pub fn burn_shares(
    mint: &AccountInfo,
    account: &AccountInfo,
    authority: &AccountInfo,
    amount: u64,
    signers: &[Signer],
) -> ProgramResult {
    // SAFETY: an account's owner is not mutated during this instruction.
    let token_program = unsafe { mint.owner() };
    mint_or_burn(8, account, mint, authority, token_program, amount, signers)
}
