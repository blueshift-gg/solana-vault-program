use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{mint_shares, TokenAccount};
use crate::state::{Load, Vault};
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # CollectFees
///
/// Mint the fee shares that reports have accrued. They are already counted
/// in total shares, so minting them moves no price. Fees are paid in shares
/// only; a recipient who wants the asset redeems like any holder.
///
/// Accounts:
///
/// 1. vault:               [mut]
/// 2. share_mint:          [mut]
/// 3. recipient_shares:    [mut]           a share token account of the fee recipient
/// 4. token_program:       [executable]
/// 5. event_authority:
/// 6. program:             [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Vault: writable, deserialized
/// - ShareMint: equal to the vault's
/// - RecipientShares: a token account owned by the vault's fee recipient
///
/// Event Data:
/// - discriminator: u8, (255u8, 42u8)
/// - vault: Pubkey,
/// - shares: u64,
pub struct CollectFees<'a> {
    pub vault: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub recipient_shares: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for CollectFees<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("CollectFees");

        let [vault, share_mint, recipient_shares, _token_program, event_authority, program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let vault_data = Vault::load(vault)?;
        if vault_data.share_mint().ne(share_mint.key())
            // SAFETY: an account's owner is not mutated during this instruction.
            || TokenAccount::load(recipient_shares, unsafe { share_mint.owner() })?
                .owner
                .ne(vault_data.config.fee_recipient())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            vault,
            share_mint,
            recipient_shares,
            event_authority,
            program,
        })
    }
}

impl<'a> CollectFees<'a> {
    pub const DISCRIMINATOR: &'a u8 = &42;

    pub fn process(&mut self) -> ProgramResult {
        let vault = Vault::load_mut(self.vault)?;
        let shares = vault.fee_shares();
        if shares == 0 {
            return Err(VaultError::ZeroAmount.into());
        }
        vault.set_fee_shares(0);

        // Mint the accrued shares, signed by the vault
        let seeds = vault.signer_seeds();
        let seeds = seeds.as_seeds();
        mint_shares(
            self.share_mint,
            self.recipient_shares,
            self.vault,
            shares,
            &[Signer::from(&seeds)],
        )?;

        // Log the CollectFees Event
        emit(
            self.event_authority,
            self.program,
            Self::DISCRIMINATOR,
            &[self.vault.key(), &shares.to_le_bytes()],
        )
    }
}
