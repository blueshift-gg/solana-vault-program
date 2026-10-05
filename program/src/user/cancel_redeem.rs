use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{close, decimals, transfer};
use crate::state::{Load, Ticket, Vault};
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # CancelRedeem
///
/// Change your mind: close a ticket and take its remaining shares back. They
/// were priced with the vault the whole time, so cancelling costs and gains
/// nothing. Works in every vault status, so shares can never be stuck in escrow.
///
/// > Transfer the remaining shares out of escrow
/// > Close the Ticket account and refund its rent
///
/// Accounts:
///
/// 1. owner:               [signer]
/// 2. payer:               [mut]           paid the ticket's rent, receives it back
/// 3. vault:
/// 4. ticket:              [mut]
/// 5. share_mint:
/// 6. escrow_account:      [mut]
/// 7. owner_shares:        [mut]
/// 8. token_program:       [executable]    the share mint's
/// 9. event_authority:
/// 10. program:            [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Owner: signer, the ticket's owner
/// - Payer: writable, the ticket's payer
/// - Vault: deserialized, the ticket's vault
/// - Ticket: writable, deserialized
/// - ShareMint, EscrowAccount: equal to the vault's
/// - OwnerShares: not the escrow itself; otherwise no need to check since the transfer fails on a wrong mint
///
/// Event Data:
/// - discriminator: u8, (255u8, 32u8)
/// - vault: Pubkey,
/// - ticket: Pubkey,
/// - shares: u64,
pub struct CancelRedeem<'a> {
    pub owner: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub ticket: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub escrow_account: &'a AccountInfo,
    pub owner_shares: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for CancelRedeem<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("CancelRedeem");

        let [owner, payer, vault, ticket, share_mint, escrow_account, owner_shares, _token_program, event_authority, program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !owner.is_signer() {
            return Err(VaultError::NotSigner.into());
        }
        if !ticket.is_writable() || !payer.is_writable() {
            return Err(VaultError::NotMutable.into());
        }
        let ticket_data = Ticket::load(ticket)?;
        if ticket_data.owner().ne(owner.key())
            || ticket_data.payer().ne(payer.key())
            || ticket_data.vault().ne(vault.key())
        {
            return Err(VaultError::InvalidTicket.into());
        }
        let vault_data = Vault::load(vault)?;
        if vault_data.share_mint().ne(share_mint.key())
            || vault_data.escrow_account().ne(escrow_account.key())
            || owner_shares.key().eq(escrow_account.key())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            owner,
            payer,
            vault,
            ticket,
            share_mint,
            escrow_account,
            owner_shares,
            event_authority,
            program,
        })
    }
}

impl<'a> CancelRedeem<'a> {
    pub const DISCRIMINATOR: &'a u8 = &32;

    pub fn process(&mut self) -> ProgramResult {
        let shares = Ticket::load(self.ticket)?.shares();

        // Release the shares, signed by the vault
        let vault = Vault::load(self.vault)?;
        let seeds = vault.signer_seeds();
        let seeds = seeds.as_seeds();
        transfer(
            // SAFETY: an account's owner is not mutated during this instruction.
            unsafe { self.share_mint.owner() },
            self.escrow_account,
            self.share_mint,
            self.owner_shares,
            self.vault,
            shares,
            decimals(self.share_mint)?,
            &[Signer::from(&seeds)],
        )?;

        close(self.ticket, self.payer)?;

        // Log the CancelRedeem Event
        emit(
            self.event_authority,
            self.program,
            Self::DISCRIMINATOR,
            &[self.vault.key(), self.ticket.key(), &shares.to_le_bytes()],
        )
    }
}
