use super::{amount, lending_data, MarketAccounts, PositionAccounts};
use crate::constants::{LENDING_DEPOSIT, LENDING_PROGRAM};
use pinocchio::cpi::invoke;
use pinocchio::instruction::{AccountMeta, Instruction};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_DEPOSIT;

/// # Deposit
///
/// Supply to the market: the lending program takes `amount` from the strategy
/// account and mints fTokens to the strategy authority's fToken account.
///
/// The lending program checks every account against the market itself. The
/// three this adapter pins are the signer, the strategy account and the
/// fToken account, so funds only ever move between those two accounts and
/// the market.
///
/// > Call the lending program's `deposit`, with the strategy authority's signature
///
/// Accounts: see `PositionAccounts`, then `MarketAccounts`.
///
/// Parameters:
/// 1. amount: u64,
pub struct Deposit<'a> {
    pub position: PositionAccounts<'a>,
    pub market: MarketAccounts<'a>,
    pub amount: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Deposit");

        let position = PositionAccounts::try_from(accounts)?;
        let market = MarketAccounts::try_from(accounts)?;
        let amount = amount(data)?;

        // Return the initialized struct
        Ok(Self {
            position,
            market,
            amount,
        })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_DEPOSIT;

    pub fn process(&mut self) -> ProgramResult {
        let (p, m) = (&self.position, &self.market);

        // The lending program's `Deposit` account list
        invoke(
            &Instruction {
                program_id: &LENDING_PROGRAM,
                accounts: &[
                    AccountMeta::writable_signer(p.strategy_authority.key()),
                    AccountMeta::writable(p.strategy_account.key()),
                    AccountMeta::writable(p.f_token_account.key()),
                    AccountMeta::readonly(p.asset_mint.key()),
                    AccountMeta::readonly(m.lending_admin.key()),
                    AccountMeta::writable(p.lending.key()),
                    AccountMeta::writable(m.f_token_mint.key()),
                    AccountMeta::writable(m.token_reserve.key()),
                    AccountMeta::writable(m.supply_position.key()),
                    AccountMeta::readonly(m.rate_model.key()),
                    AccountMeta::writable(m.liquidity_vault.key()),
                    AccountMeta::writable(m.liquidity.key()),
                    AccountMeta::readonly(m.liquidity_program.key()),
                    AccountMeta::readonly(m.rewards_rate_model.key()),
                    AccountMeta::readonly(p.token_program.key()),
                    AccountMeta::readonly(m.associated_token_program.key()),
                    AccountMeta::readonly(m.system_program.key()),
                ],
                data: &lending_data(&LENDING_DEPOSIT, self.amount),
            },
            &[
                p.strategy_authority,
                p.strategy_account,
                p.f_token_account,
                p.asset_mint,
                m.lending_admin,
                p.lending,
                m.f_token_mint,
                m.token_reserve,
                m.supply_position,
                m.rate_model,
                m.liquidity_vault,
                m.liquidity,
                m.liquidity_program,
                m.rewards_rate_model,
                p.token_program,
                m.associated_token_program,
                m.system_program,
            ],
        )
    }
}
