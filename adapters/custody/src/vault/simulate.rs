use super::BookAccounts;
use crate::constants::REPORT_LEN;
use crate::errors::CustodyError;
use crate::helpers::now;
use crate::state::{report_message, Custody};
use pinocchio::cpi::set_return_data;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use vault_core::constants::ADAPTER_SIMULATE;

/// # Simulate
///
/// Say what the book is worth: its units at the oracle's price, plus
/// returned funds already settled. A new price can ride along: the oracle
/// signs it off chain and anyone delivers it here. Without one, the stored
/// price is used for as long as it lasts.
///
/// > Adopt the price report, if one was delivered
/// > Return `units × price + settled`
///
/// Accounts: see `BookAccounts`.
///
/// Parameters:
/// 1. report: [u8; 80],            // optional: price, expires_at, then the signature
///
/// Instruction Checks:
/// - Report: signed by the oracle, not expired, and newer than the stored price
///
/// Return Data:
/// - value: u64,
pub struct Simulate<'a> {
    pub accounts: BookAccounts<'a>,
    pub report: Option<&'a [u8; REPORT_LEN]>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Simulate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Simulate");

        let accounts = BookAccounts::try_from(accounts)?;
        let report = match data.len() {
            0 => None,
            REPORT_LEN => Some(data.try_into().unwrap()),
            _ => return Err(CustodyError::InvalidReport.into()),
        };

        Ok(Self { accounts, report })
    }
}

impl<'a> Simulate<'a> {
    pub const DISCRIMINATOR: &'a [u8; 8] = &ADAPTER_SIMULATE;

    pub fn process(&mut self) -> ProgramResult {
        let now = now()?;
        let custody = Custody::load_mut(self.accounts.custody)?;

        if let Some(report) = self.report {
            let price = u64::from_le_bytes(report[..8].try_into().unwrap());
            let expires_at = i64::from_le_bytes(report[8..16].try_into().unwrap());
            let signature: &[u8; 64] = report[16..].try_into().unwrap();

            // Still valid, and newer than every report already accepted. The
            // last one can be presented again as long as it changes nothing:
            // the same expiry with another price, or over a book that has
            // since restarted at par, is a replay.
            let stored = custody.price_expires_at();
            let newer = expires_at > stored || (expires_at == stored && price == custody.price());
            if now > expires_at || !newer {
                return Err(CustodyError::StaleReport.into());
            }
            // Signed by the oracle, which may have just taken over
            custody.promote_oracle(now);
            brine_ed25519::verify_strict(
                &brine_ed25519::Address::new_from_array(*custody.oracle()),
                signature,
                &[&report_message(
                    custody.strategy_authority(),
                    custody.book(),
                    price,
                    expires_at,
                )],
            )
            .map_err(|_| CustodyError::InvalidReport)?;
            custody.set_price(price, expires_at);
        }

        let price = custody.live_price(now)?;
        set_return_data(&custody.value(price)?.to_le_bytes());
        Ok(())
    }
}
