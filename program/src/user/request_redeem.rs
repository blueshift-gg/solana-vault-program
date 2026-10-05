use crate::constants::{TICKET_LEN, TICKET_SEED};
use crate::errors::VaultError;
use crate::events::emit;
use crate::helpers::{
    burn_shares, check_uninitialized, clock, create_pda, decimals, receive, transfer, TokenAccount,
};
use crate::state::{Load, Ticket, Vault};
use core::mem::size_of;
use pinocchio::instruction::Signer;
use pinocchio::log::sol_log;
use pinocchio::pubkey::create_program_address;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # RequestRedeem
///
/// Ask to leave. If idle covers the whole request at a fresh price, it is
/// paid on the spot: the shares are burned and the assets sent, and no
/// ticket is created. Otherwise the shares move into escrow under a ticket
/// and keep earning and losing with the vault until `Fulfil` pays them out.
/// Works in every vault status.
///
/// A request waits as a ticket whenever it cannot be paid in full right now:
/// idle is short, the price is stale, or the vault has a withdrawal authority
/// whose permit a new ticket would need.
///
/// > Idle covers it: burn the shares and pay the owner from idle
/// > Otherwise: create the Ticket at its seed-derived PDA and escrow the shares
///
/// Accounts:
///
/// 1. payer:               [signer, mut]   pays a ticket's rent, refunded when it closes
/// 2. owner:               [signer]        owns the shares and receives the payout
/// 3. vault:               [mut]
/// 4. ticket:              [mut]           PDA [TICKET_SEED, vault, owner, id]
/// 5. share_mint:          [mut]
/// 6. owner_shares:        [mut]
/// 7. escrow_account:      [mut]
/// 8. idle_account:        [mut]
/// 9. asset_mint:
/// 10. destination:        [mut]           an asset token account of the owner
/// 11. share_token_program:[executable]
/// 12. asset_token_program:[executable]
/// 13. system_program:     [executable]
/// 14. event_authority:
/// 15. program:            [executable]    this program, for the event CPI
///
/// Parameters:
/// 1. shares: u64,
/// 2. id: u64,                     // chosen by the client; one owner can hold many tickets
/// 3. bump: u8,                    // ticket PDA bump, one fixed-cost derivation
///
/// Account Checks:
/// - Payer, Owner: signers
/// - Vault: writable, deserialized
/// - Ticket: writable, empty system account; the PDA check needs the id, so it runs in process
/// - ShareMint, EscrowAccount, IdleAccount, AssetMint: equal to the vault's
/// - OwnerShares: no need to check since the burn or the transfer fails on a wrong mint or owner
/// - Destination: a token account owned by the owner; only read when paying on the spot
///
/// Instruction Checks:
/// - Shares: nonzero
///
/// Event Data:
/// - discriminator: u8, (255u8, 31u8)
/// - vault: Pubkey,
/// - ticket: Pubkey,               // all zeros when paid on the spot
/// - owner: Pubkey,
/// - shares: u64,                  // burned on the spot, or escrowed
/// - assets: u64,                  // paid on the spot; zero when a ticket was created
pub struct RequestRedeemAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub owner: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub ticket: &'a AccountInfo,
    pub share_mint: &'a AccountInfo,
    pub owner_shares: &'a AccountInfo,
    pub escrow_account: &'a AccountInfo,
    pub idle_account: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub destination: &'a AccountInfo,
    pub event_authority: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for RequestRedeemAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, owner, vault, ticket, share_mint, owner_shares, escrow_account, idle_account, asset_mint, destination, _share_token_program, _asset_token_program, _system_program, event_authority, program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !payer.is_signer() || !owner.is_signer() {
            return Err(VaultError::NotSigner.into());
        }
        if !ticket.is_writable() {
            return Err(VaultError::NotMutable.into());
        }
        check_uninitialized(ticket)?;
        let vault_data = Vault::load(vault)?;
        if vault_data.share_mint().ne(share_mint.key())
            || vault_data.escrow_account().ne(escrow_account.key())
            || vault_data.idle_account().ne(idle_account.key())
            || vault_data.asset_mint().ne(asset_mint.key())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            payer,
            owner,
            vault,
            ticket,
            share_mint,
            owner_shares,
            escrow_account,
            idle_account,
            asset_mint,
            destination,
            event_authority,
            program,
        })
    }
}

pub struct RequestRedeemInstructionData {
    pub shares: u64,
    pub id: u64,
    pub bump: u8,
}

impl TryFrom<&[u8]> for RequestRedeemInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        if data.len().ne(&(2 * size_of::<u64>() + size_of::<u8>())) {
            return Err(ProgramError::InvalidInstructionData);
        }

        let shares = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let id = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let bump = data[16];

        // Instruction Checks
        if shares == 0 {
            return Err(VaultError::ZeroAmount.into());
        }

        Ok(Self { shares, id, bump })
    }
}

pub struct RequestRedeem<'a> {
    pub accounts: RequestRedeemAccounts<'a>,
    pub instruction_data: RequestRedeemInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for RequestRedeem<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("RequestRedeem");

        let accounts = RequestRedeemAccounts::try_from(accounts)?;
        let instruction_data = RequestRedeemInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> RequestRedeem<'a> {
    pub const DISCRIMINATOR: &'a u8 = &31;

    pub fn process(&mut self) -> ProgramResult {
        let (slot, now) = clock()?;
        let vault = Vault::load_mut(self.accounts.vault)?;

        // Paid on the spot there is no ticket, so the event names none
        let (ticket, shares, assets) =
            match vault.redeem(self.instruction_data.shares, slot, now)? {
                Some(assets) => (&[0; 32], self.pay(vault, assets)?, assets),
                None => (self.accounts.ticket.key(), self.queue(vault, now)?, 0),
            };

        // Log the RequestRedeem Event
        emit(
            self.accounts.event_authority,
            self.accounts.program,
            Self::DISCRIMINATOR,
            &[
                self.accounts.vault.key(),
                ticket,
                self.accounts.owner.key(),
                &shares.to_le_bytes(),
                &assets.to_le_bytes(),
            ],
        )
    }

    /// Idle covers the request: burn the owner's shares and pay the owner.
    fn pay(&self, vault: &Vault, assets: u64) -> Result<u64, ProgramError> {
        let shares = self.instruction_data.shares;
        if TokenAccount::load(self.accounts.destination, vault.asset_token_program())?
            .owner
            .ne(self.accounts.owner.key())
        {
            return Err(VaultError::InvalidTokenAccount.into());
        }

        // The owner signed this transaction, so it authorizes the burn
        burn_shares(
            self.accounts.share_mint,
            self.accounts.owner_shares,
            self.accounts.owner,
            shares,
            &[],
        )?;

        // Pay from idle, signed by the vault
        let seeds = vault.signer_seeds();
        let seeds = seeds.as_seeds();
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
        Ok(shares)
    }

    /// The request has to wait: escrow the shares under a ticket.
    fn queue(&self, vault: &mut Vault, now: i64) -> Result<u64, ProgramError> {
        let vault_key = self.accounts.vault.key();
        let owner_key = self.accounts.owner.key();
        let id = self.instruction_data.id;

        // The ticket must be the PDA for (vault, owner, id, bump)
        let ticket_key = create_program_address(
            &[
                TICKET_SEED,
                vault_key,
                owner_key,
                &id.to_le_bytes(),
                &[self.instruction_data.bump],
            ],
            &crate::ID,
        )
        .map_err(|_| VaultError::InvalidSeeds)?;
        if ticket_key.ne(self.accounts.ticket.key()) {
            return Err(VaultError::InvalidSeeds.into());
        }

        // Create the Ticket account
        let seeds = Ticket::seeds(vault_key, owner_key, id, self.instruction_data.bump);
        create_pda(
            self.accounts.payer,
            self.accounts.ticket,
            TICKET_LEN,
            &seeds.as_seeds(),
        )?;

        // Escrow the shares and count what arrives: a share mint may
        // withhold a fee of its own
        // SAFETY: an account's owner is not mutated during this instruction.
        let share_token_program = unsafe { self.accounts.share_mint.owner() };
        let shares = receive(
            share_token_program,
            self.accounts.owner_shares,
            self.accounts.share_mint,
            self.accounts.escrow_account,
            self.accounts.owner,
            self.instruction_data.shares,
            decimals(self.accounts.share_mint)?,
            &[],
        )?;
        if shares == 0 {
            return Err(VaultError::ZeroAmount.into());
        }

        // Populate the ticket with them
        Ticket::load_new(self.accounts.ticket)?.set_inner(
            self.instruction_data.bump,
            vault_key,
            owner_key,
            self.accounts.payer.key(),
            shares,
            now,
            vault.next_ticket()?,
        );
        Ok(shares)
    }
}
