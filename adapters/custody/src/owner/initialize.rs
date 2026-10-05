use crate::constants::{CUSTODY_LEN, CUSTODY_SEED, INITIALIZE};
use crate::errors::CustodyError;
use crate::helpers::{load_vault, TokenAccount};
use crate::state::Custody;
use pinocchio::instruction::{Seed, Signer};
use pinocchio::log::sol_log;
use pinocchio::pubkey::{find_program_address, Pubkey};
use pinocchio::sysvars::{rent::Rent, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Initialize
///
/// Open the book for one vault: name where its funds go out, where they come
/// back, and who prices them. The vault's owner does it, once. The two token
/// accounts cannot be changed afterwards; a different custodian is a
/// different vault.
///
/// > Create the Custody account at its seed-derived PDA
/// > Record the vault, the destination, the return account and the oracle
///
/// Accounts:
///
/// 1. payer:               [signer, mut]   pays rent
/// 2. owner:               [signer]        the vault's owner
/// 3. vault:                               a vault whose adapter is this program
/// 4. custody:             [mut]           PDA [CUSTODY_SEED, vault's strategy authority]
/// 5. destination:                         the custodian's token account for the asset
/// 6. return_account:                      a token account for the asset owned by the custody
/// 7. system_program:      [executable]
///
/// Parameters:
/// 1. oracle: Pubkey,              // signs price reports
///
/// Account Checks:
/// - Payer: signer
/// - Owner: signer, the vault's owner
/// - Vault: a vault of the vault program whose adapter is this program
/// - Custody: writable, empty system account, at its PDA with the canonical bump, so a vault has exactly one
/// - Destination: an initialized token account of the vault's asset
/// - ReturnAccount: the same, owned by the custody, no delegate, no close authority
pub struct InitializeAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub owner: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub custody: &'a AccountInfo,
    pub destination: &'a AccountInfo,
    pub return_account: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, owner, vault, custody, destination, return_account, _system_program] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !payer.is_signer() || !owner.is_signer() {
            return Err(CustodyError::NotSigner.into());
        }
        if !custody.is_writable() {
            return Err(CustodyError::NotMutable.into());
        }
        if !custody.is_owned_by(&pinocchio_system::ID) || custody.data_len() != 0 {
            return Err(CustodyError::InvalidCustody.into());
        }
        let vault_data = load_vault(vault)?;
        if vault_data.owner().ne(owner.key()) {
            return Err(CustodyError::InvalidOwner.into());
        }
        let token_program = vault_data.asset_token_program();
        let out = TokenAccount::load(destination, token_program)?;
        let back = TokenAccount::load(return_account, token_program)?;
        if out.mint.ne(vault_data.asset_mint())
            || !out.is_usable()
            || back.mint.ne(vault_data.asset_mint())
            || !back.is_usable()
            || !back.is_clean()
            || back.owner.ne(custody.key())
        {
            return Err(CustodyError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            payer,
            owner,
            vault,
            custody,
            destination,
            return_account,
        })
    }
}

pub struct InitializeInstructionData<'a> {
    pub oracle: &'a Pubkey,
}

impl<'a> TryFrom<&'a [u8]> for InitializeInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            oracle: data
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        })
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Initialize");

        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &INITIALIZE;

    pub fn process(&mut self) -> ProgramResult {
        let custody = self.accounts.custody;
        let strategy_authority = *load_vault(self.accounts.vault)?.strategy_authority();

        // The custody must be the one PDA for the vault's strategy authority:
        // the canonical bump, so no second book can be opened beside it
        let (expected, bump) =
            find_program_address(&[CUSTODY_SEED, &strategy_authority], &crate::ID);
        if expected.ne(custody.key()) {
            return Err(CustodyError::InvalidCustody.into());
        }
        let bump = [bump];

        // Create the Custody account; anyone can send lamports to a PDA
        // before initialization
        let seeds = [
            Seed::from(CUSTODY_SEED),
            Seed::from(&strategy_authority),
            Seed::from(&bump),
        ];
        let signer = [Signer::from(&seeds)];
        let missing = Rent::get()?
            .minimum_balance(CUSTODY_LEN)
            .saturating_sub(custody.lamports());
        if missing > 0 {
            pinocchio_system::instructions::Transfer {
                from: self.accounts.payer,
                to: custody,
                lamports: missing,
            }
            .invoke()?;
        }
        pinocchio_system::instructions::Allocate {
            account: custody,
            space: CUSTODY_LEN as u64,
        }
        .invoke_signed(&signer)?;
        pinocchio_system::instructions::Assign {
            account: custody,
            owner: &crate::ID,
        }
        .invoke_signed(&signer)?;

        // Populate it: an empty book at par
        Custody::load_new(custody)?.set_inner(
            bump[0],
            &strategy_authority,
            self.accounts.vault.key(),
            self.accounts.destination.key(),
            self.accounts.return_account.key(),
            self.instruction_data.oracle,
        );
        Ok(())
    }
}
