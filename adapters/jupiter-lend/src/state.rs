//! This adapter stores nothing. Its state is a view: the vault's position,
//! read from the lending program's market account and the strategy
//! authority's fToken account.

use crate::{constants::*, errors::AdapterError, vault::token_balance};
use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::find_program_address,
};

/// Assets that `shares` fTokens redeem for at `price`, rounded down as the
/// lending program rounds a redemption.
#[inline]
pub fn to_assets(shares: u64, price: u64) -> Option<u64> {
    u64::try_from(shares as u128 * price as u128 / EXCHANGE_PRICES_PRECISION).ok()
}

/// A vault's position in one market.
pub struct Position {
    /// fTokens held.
    pub shares: u64,
    /// Assets per fToken as the lending account last stored it, scaled by
    /// `EXCHANGE_PRICES_PRECISION`. It only moves when someone uses the
    /// market, and it never falls, so a value from it is exact or slightly low:
    /// run the lending program's permissionless `update_rate` in the same
    /// transaction for the exact one.
    pub price: u64,
}

impl Position {
    /// Read the position after pinning both accounts that define it: the
    /// lending program's own market for this asset, and the one fToken
    /// account nobody else can stand in for. Anyone can open another fToken
    /// account for the authority; only its associated token account counts.
    pub fn load(
        strategy_authority: &AccountInfo,
        asset_mint: &AccountInfo,
        token_program: &AccountInfo,
        f_token_account: &AccountInfo,
        lending: &AccountInfo,
    ) -> Result<Self, ProgramError> {
        // SAFETY: nothing in this program holds a mutable borrow of account data.
        let data = unsafe { lending.borrow_data_unchecked() };
        if !lending.is_owned_by(&LENDING_PROGRAM)
            || data.len() < LENDING_TOKEN_EXCHANGE_PRICE + 8
            || data[..8] != LENDING_ACCOUNT
            || data[LENDING_MINT..LENDING_MINT + 32] != asset_mint.key()[..]
        {
            return Err(AdapterError::InvalidLending.into());
        }
        let f_token_mint = &data[LENDING_F_TOKEN_MINT..LENDING_F_TOKEN_MINT + 32];
        let price = u64::from_le_bytes(
            data[LENDING_TOKEN_EXCHANGE_PRICE..LENDING_TOKEN_EXCHANGE_PRICE + 8]
                .try_into()
                .unwrap(),
        );

        let (expected, _) = find_program_address(
            &[strategy_authority.key(), token_program.key(), f_token_mint],
            &pinocchio_associated_token_account::ID,
        );
        if expected.ne(f_token_account.key()) {
            return Err(AdapterError::InvalidFTokenAccount.into());
        }
        // Not created yet is an empty position
        let shares = if f_token_account.is_owned_by(token_program.key()) {
            token_balance(f_token_account)
        } else {
            0
        };

        Ok(Self { shares, price })
    }

    /// What the position redeems for right now.
    #[inline]
    pub fn value(&self) -> Result<u64, ProgramError> {
        to_assets(self.shares, self.price).ok_or(AdapterError::MathOverflow.into())
    }
}
