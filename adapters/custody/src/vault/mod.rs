pub mod deposit;
pub use deposit::*;

pub mod simulate;
pub use simulate::*;

pub mod withdraw;
pub use withdraw::*;

use crate::errors::CustodyError;
use crate::state::Custody;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// The accounts of every instruction the vault program calls: the interface
/// prefix (`vault_core::constants`), then this adapter's own.
///
/// `Deposit` and `Withdraw` move funds, so they also require the strategy
/// authority's signature (`check_signed`), which only the vault can give.
/// Without it anyone could call them directly with accounts of their own.
///
/// 1. strategy_authority:  [signer on deposit and withdraw]
/// 2. strategy_account:    [mut]
/// 3. asset_mint:
/// 4. token_program:       [executable]
/// 5. custody:             [mut]           PDA [CUSTODY_SEED, strategy_authority]
/// 6. return_account:      [mut]           the custody's token account; funds come back here
/// 7. destination:         [mut]           the custodian's token account; funds go out here
///
/// Account Checks:
/// - Custody: deserialized, set up for this strategy authority
/// - ReturnAccount, Destination: equal to the custody's
/// - StrategyAuthority: the custody's; a signer on `Deposit` and `Withdraw`
/// - StrategyAccount, AssetMint: no need to check since only the vault can sign, and it passes its own
pub struct BookAccounts<'a> {
    pub strategy_authority: &'a AccountInfo,
    pub strategy_account: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub custody: &'a AccountInfo,
    pub return_account: &'a AccountInfo,
    pub destination: &'a AccountInfo,
}

impl BookAccounts<'_> {
    /// The vault signed for this call with its strategy authority.
    #[inline(always)]
    pub fn check_signed(&self) -> Result<(), ProgramError> {
        if !self.strategy_authority.is_signer() {
            return Err(CustodyError::NotSigner.into());
        }
        Ok(())
    }
}

impl<'a> TryFrom<&'a [AccountInfo]> for BookAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [strategy_authority, strategy_account, asset_mint, token_program, custody, return_account, destination, ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let custody_data = Custody::load(custody)?;
        if custody_data
            .strategy_authority()
            .ne(strategy_authority.key())
        {
            return Err(CustodyError::InvalidCustody.into());
        }
        if custody_data.return_account().ne(return_account.key())
            || custody_data.destination().ne(destination.key())
        {
            return Err(CustodyError::InvalidTokenAccount.into());
        }

        // Return the accounts
        Ok(Self {
            strategy_authority,
            strategy_account,
            asset_mint,
            token_program,
            custody,
            return_account,
            destination,
        })
    }
}
