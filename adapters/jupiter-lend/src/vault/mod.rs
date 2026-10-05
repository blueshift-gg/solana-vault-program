pub mod deposit;
pub use deposit::*;

pub mod simulate;
pub use simulate::*;

pub mod withdraw;
pub use withdraw::*;

use crate::constants::LENDING_PROGRAM;
use crate::errors::AdapterError;
use crate::state::Position;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// The accounts that define a vault's position: the interface prefix
/// (`vault_core::constants`), then this adapter's first two.
///
/// 1. strategy_authority:  [signer, mut on deposit and withdraw]
/// 2. strategy_account:    [mut]           the strategy authority's ATA for the asset
/// 3. asset_mint:
/// 4. token_program:       [executable]
/// 5. f_token_account:     [mut]           the strategy authority's ATA for the fToken
/// 6. lending:             [mut]           the market for the asset mint
///
/// Account Checks:
/// - Lending: owned by the lending program, a `Lending` account, for this asset mint
/// - FTokenAccount: the strategy authority's associated token account for the market's fToken
/// - StrategyAuthority, StrategyAccount: no need to check since the lending program does
pub struct PositionAccounts<'a> {
    pub strategy_authority: &'a AccountInfo,
    pub strategy_account: &'a AccountInfo,
    pub asset_mint: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub f_token_account: &'a AccountInfo,
    pub lending: &'a AccountInfo,
    pub position: Position,
}

impl<'a> TryFrom<&'a [AccountInfo]> for PositionAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [strategy_authority, strategy_account, asset_mint, token_program, f_token_account, lending, ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        let position = Position::load(
            strategy_authority,
            asset_mint,
            token_program,
            f_token_account,
            lending,
        )?;

        // Return the accounts
        Ok(Self {
            strategy_authority,
            strategy_account,
            asset_mint,
            token_program,
            f_token_account,
            lending,
            position,
        })
    }
}

/// The rest of what the lending program's `deposit` and `withdraw` take,
/// after `PositionAccounts`:
///
/// 7. lending_admin:
/// 8. f_token_mint:        [mut]
/// 9. token_reserve:       [mut]
/// 10. supply_position:    [mut]
/// 11. rate_model:
/// 12. liquidity_vault:    [mut]
/// 13. liquidity:          [mut]
/// 14. liquidity_program:  [executable]
/// 15. rewards_rate_model:
/// 16. associated_token_program: [executable]
/// 17. system_program:     [executable]
/// 18. lending_program:    [executable]
///
/// Account Checks:
/// - LendingProgram: the lending program
/// - The rest: no need to check since the lending program checks each against the market
pub struct MarketAccounts<'a> {
    pub lending_admin: &'a AccountInfo,
    pub f_token_mint: &'a AccountInfo,
    pub token_reserve: &'a AccountInfo,
    pub supply_position: &'a AccountInfo,
    pub rate_model: &'a AccountInfo,
    pub liquidity_vault: &'a AccountInfo,
    pub liquidity: &'a AccountInfo,
    pub liquidity_program: &'a AccountInfo,
    pub rewards_rate_model: &'a AccountInfo,
    pub associated_token_program: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub lending_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for MarketAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [_, _, _, _, _, _, lending_admin, f_token_mint, token_reserve, supply_position, rate_model, liquidity_vault, liquidity, liquidity_program, rewards_rate_model, associated_token_program, system_program, lending_program, ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if lending_program.key().ne(&LENDING_PROGRAM) {
            return Err(AdapterError::InvalidLendingProgram.into());
        }

        // Return the accounts
        Ok(Self {
            lending_admin,
            f_token_mint,
            token_reserve,
            supply_position,
            rate_model,
            liquidity_vault,
            liquidity,
            liquidity_program,
            rewards_rate_model,
            associated_token_program,
            system_program,
            lending_program,
        })
    }
}

/// `amount` as the lending program's instruction data.
#[inline(always)]
pub(crate) fn lending_data(discriminator: &[u8; 8], amount: u64) -> [u8; 16] {
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(discriminator);
    data[8..].copy_from_slice(&amount.to_le_bytes());
    data
}

/// The balance of a token account, or zero if it is not one. The lending
/// program checks the account itself; this only sizes a request.
#[inline(always)]
pub(crate) fn token_balance(account: &AccountInfo) -> u64 {
    // SAFETY: nothing in this program holds a mutable borrow of account data.
    unsafe { account.borrow_data_unchecked() }
        .get(64..72)
        .map_or(0, |amount| u64::from_le_bytes(amount.try_into().unwrap()))
}

/// The amount the vault asked for: the first eight bytes of the data. The
/// vault may append opaque bytes; this adapter takes none.
#[inline(always)]
pub(crate) fn amount(data: &[u8]) -> Result<u64, ProgramError> {
    data.split_first_chunk()
        .map(|(amount, _)| u64::from_le_bytes(*amount))
        .ok_or(ProgramError::InvalidInstructionData)
}
