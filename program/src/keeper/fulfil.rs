use crate::adapter::Adapter;
use crate::constants::{PERMIT_FULFIL, PERMIT_LEN};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{burn_shares, check_permit, clock, close, transfer, TokenAccount};
use crate::state::{Load, Ticket, Vault};
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Fulfil
///
/// Pay a ticket: burn its shares and send their worth to the owner, at the
/// price right now, which must be fresh. Idle pays first. If idle is short,
/// passing the adapter accounts pulls the difference out of the strategy in
/// the same instruction. A ticket idle still cannot cover is paid in part and
/// stays open for the rest.
///
/// Anyone can fulfil any ticket. If the vault has a withdrawal authority, a
/// ticket younger than the fulfil delay also needs that authority's permit
/// for that very ticket; once the delay has passed, nobody's permission is
/// needed.
///
/// > Pull the shortfall from the adapter and sweep it into idle
/// > Burn the escrowed shares and pay the owner from idle
/// > Close the Ticket account once it is empty and refund its rent
///
/// Accounts:
///
/// 1. vault:               [mut]
/// 2. ticket:              [mut]
/// 3. payer:               [mut]           paid the ticket's rent, receives it back
/// 4. escrow_account:      [mut]
/// 5. share_mint:          [mut]
/// 6. idle_account:        [mut]
/// 7. asset_mint:
/// 8. destination:         [mut]           an asset token account of the ticket's owner
/// 9. asset_token_program: [executable]
/// 10. share_token_program:[executable]
/// 11. event_authority:
/// 12. program:            [executable]    this program, for the event CPI
/// 13. ..                                  optional: see `Adapter`
///
/// Parameters:
/// 1. has_permit: u8,              // 1 if a permit follows
/// 2. permit: [u8; 72],            // only if has_permit: expires_at, then the signature
/// 3. data: [u8],                  // opaque, forwarded to the adapter
///
/// Account Checks:
/// - Vault: writable, deserialized
/// - Ticket: writable, deserialized, of this vault and payer
/// - Payer: writable
/// - EscrowAccount, ShareMint, IdleAccount, AssetMint: equal to the vault's
/// - Destination: a token account owned by the ticket's owner
/// - Adapter accounts: checked by `Adapter::load`
///
/// Event Data:
/// - discriminator: u8, (255u8, 41u8)
/// - vault: Pubkey,
/// - ticket: Pubkey,
/// - assets: u64,
/// - shares: u64,
pub struct FulfilAccounts<'a> {
    pub vault: &'a AccountInfo,
    pub ticket: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub escrow_account: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub destination: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub adapter: Option<Adapter<'a>>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for FulfilAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [vault, ticket, payer, escrow_account, share_mint, idle_account, asset_mint, destination, _asset_token_program, _share_token_program, event_authority, program, adapter @ ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !ticket.is_writable() || !payer.is_writable() {
            return Err(VaultError::NotMutable.into());
        }
        let vault_data = Vault::load(vault)?;
        if vault_data.escrow_account().ne(escrow_account.key())
            || vault_data.share_mint().ne(share_mint.key())
            || vault_data.idle_account().ne(idle_account.key())
            || vault_data.asset_mint().ne(asset_mint.key())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        let ticket_data = Ticket::load(ticket)?;
        if ticket_data.vault().ne(vault.key()) || ticket_data.payer().ne(payer.key()) {
            return Err(VaultError::InvalidTicket.into());
        }
        if TokenAccount::load(destination, vault_data.asset_token_program())?
            .owner
            .ne(ticket_data.owner())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }
        let adapter = match adapter {
            [] => None,
            accounts => Some(Adapter::load(accounts, vault_data)?),
        };

        // Return the accounts
        Ok(Self {
            vault,
            ticket,
            payer,
            escrow_account,
            share_mint,
            idle_account,
            asset_mint,
            destination,
            event_authority,
            program,
            adapter,
        })
    }
}

pub struct FulfilInstructionData<'a> {
    pub permit: Option<&'a [u8; PERMIT_LEN]>,
    pub data: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for FulfilInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (permit, data) = match data.split_first() {
            Some((0, data)) => (None, data),
            Some((1, data)) => {
                let (permit, data) = data
                    .split_first_chunk()
                    .ok_or(ProgramError::InvalidInstructionData)?;
                (Some(permit), data)
            }
            _ => return Err(ProgramError::InvalidInstructionData),
        };

        Ok(Self { permit, data })
    }
}

pub struct Fulfil<'a> {
    pub accounts: FulfilAccounts<'a>,
    pub instruction_data: FulfilInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Fulfil<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Fulfil");

        let accounts = FulfilAccounts::try_from(accounts)?;
        let instruction_data = FulfilInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Fulfil<'a> {
    pub const DISCRIMINATOR: &'a u8 = &41;

    pub fn process(&mut self) -> ProgramResult {
        let (slot, now) = clock()?;
        let ticket = Ticket::load_mut(self.accounts.ticket)?;

        // While the ticket is gated, the withdrawal authority must have permitted it
        let vault = Vault::load(self.accounts.vault)?;
        if ticket.is_gated(&vault.config, now) {
            check_permit(
                vault.config.withdraw_authority(),
                PERMIT_FULFIL,
                self.accounts.vault.key(),
                self.accounts.ticket.key(),
                ticket.nonce(),
                self.instruction_data.permit,
                now,
            )?;
        }
        vault.check_fresh(slot)?;

        // Pull what idle cannot cover from the strategy, counted as it arrives
        let owed = vault.to_assets(ticket.shares())?;
        if let (true, Some(adapter)) = (owed > vault.idle(), &self.accounts.adapter) {
            let seeds = Vault::strategy_seeds(self.accounts.vault.key(), vault.strategy_bump());
            let seeds = seeds.as_seeds();
            let signer = Signer::from(&seeds);
            adapter.withdraw(owed - vault.idle(), self.instruction_data.data, &signer)?;
            adapter.sweep(self.accounts.vault, self.accounts.idle_account, &signer)?;
        }

        // Settle against idle: all of the ticket, or as much as idle pays for
        let vault = Vault::load_mut(self.accounts.vault)?;
        let (assets, shares) = vault.fulfil(ticket.shares(), now)?;
        let remaining = ticket.shares() - shares;
        ticket.set_shares(remaining);

        // Burn the shares and pay the owner, both signed by the vault
        let seeds = vault.signer_seeds();
        let seeds = seeds.as_seeds();
        burn_shares(
            self.accounts.share_mint,
            self.accounts.escrow_account,
            self.accounts.vault,
            shares,
            &[Signer::from(&seeds)],
        )?;
        transfer(
            vault.asset_token_program(),
            self.accounts.idle_account,
            self.accounts.asset_mint,
            self.accounts.destination,
            self.accounts.vault,
            assets,
            vault.decimals(),
            &[Signer::from(&seeds)],
        )?;

        if remaining == 0 {
            close(self.accounts.ticket, self.accounts.payer)?;
        }

        // Log the Fulfil Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[
                self.accounts.vault.key(),
                self.accounts.ticket.key(),
                &assets.to_le_bytes(),
                &shares.to_le_bytes(),
            ],
        )
    }
}
