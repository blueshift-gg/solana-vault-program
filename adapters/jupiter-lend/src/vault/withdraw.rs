use super::{amount, lending_data, token_balance, MarketAccounts, PositionAccounts};
use crate::constants::{LENDING_PROGRAM, LENDING_WITHDRAW, MIN_OPERATE_AMOUNT};
use pinocchio::cpi::invoke;
use pinocchio::instruction::{AccountMeta, Instruction};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_WITHDRAW;

/// # Withdraw
///
/// Take funds out of the market: the lending program burns fTokens and sends
/// the assets to the strategy account. The vault asks for "up to `amount`",
/// so this never asks the market for more than the position redeems for or
/// than the market holds; an amount the market would refuse as too small is
/// nothing to do.
///
/// > Call the lending program's `withdraw`, with the strategy authority's signature
///
/// Accounts: see `PositionAccounts`, then `MarketAccounts`.
///
/// Parameters:
/// 1. amount: u64,
pub struct Withdraw<'a> {
    pub position: PositionAccounts<'a>,
    pub market: MarketAccounts<'a>,
    pub amount: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Withdraw");

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

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_WITHDRAW;

    pub fn process(&mut self) -> ProgramResult {
        let (p, m) = (&self.position, &self.market);
        // At least the market's own minimum, so a shortfall of a few units
        // can still be pulled; the vault takes back only what it is owed.
        // Then "up to": no more than the position redeems for, and no more
        // than the market has on hand to pay out right now.
        let amount = self
            .amount
            .max(MIN_OPERATE_AMOUNT)
            .min(p.position.value()?)
            .min(token_balance(m.liquidity_vault));
        if amount < MIN_OPERATE_AMOUNT {
            return Ok(());
        }

        // The lending program's `Withdraw` account list
        invoke(
            &Instruction {
                program_id: &LENDING_PROGRAM,
                accounts: &[
                    AccountMeta::writable_signer(p.strategy_authority.key()),
                    AccountMeta::writable(p.f_token_account.key()),
                    AccountMeta::writable(p.strategy_account.key()),
                    AccountMeta::readonly(m.lending_admin.key()),
                    AccountMeta::writable(p.lending.key()),
                    AccountMeta::readonly(p.asset_mint.key()),
                    AccountMeta::writable(m.f_token_mint.key()),
                    AccountMeta::writable(m.token_reserve.key()),
                    AccountMeta::writable(m.supply_position.key()),
                    AccountMeta::readonly(m.rate_model.key()),
                    AccountMeta::writable(m.liquidity_vault.key()),
                    // claim_account: none, which Anchor spells as the program id
                    AccountMeta::readonly(m.lending_program.key()),
                    AccountMeta::writable(m.liquidity.key()),
                    AccountMeta::readonly(m.liquidity_program.key()),
                    AccountMeta::readonly(m.rewards_rate_model.key()),
                    AccountMeta::readonly(p.token_program.key()),
                    AccountMeta::readonly(m.associated_token_program.key()),
                    AccountMeta::readonly(m.system_program.key()),
                ],
                data: &lending_data(&LENDING_WITHDRAW, amount),
            },
            &[
                p.strategy_authority,
                p.f_token_account,
                p.strategy_account,
                m.lending_admin,
                p.lending,
                p.asset_mint,
                m.f_token_mint,
                m.token_reserve,
                m.supply_position,
                m.rate_model,
                m.liquidity_vault,
                m.lending_program,
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
