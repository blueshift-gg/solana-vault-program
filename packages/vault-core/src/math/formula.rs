//! Pure integer formulas: no accounts, no syscalls, no dependencies.
//!
//! `A` is total assets (`idle + debt`) and `S` is total shares. Every
//! conversion adds one virtual asset and one virtual share, so an empty vault
//! prices 1:1 and no division is by zero. Every rounding favours the vault:
//! shares minted and assets paid round down, shares burned round up.
//!
//! `None` means the result does not fit in a `u64`; nothing wraps.

pub const BPS: u128 = 10_000;
/// 365 days, the year Morpho Vault V2 defines its rate caps over.
pub const YEAR: u128 = 365 * 24 * 60 * 60;

#[inline]
fn mul_div(a: u64, b: u128, d: u128) -> Option<u64> {
    u64::try_from((a as u128).checked_mul(b)? / d).ok()
}

#[inline]
fn mul_div_ceil(a: u64, b: u128, d: u128) -> Option<u64> {
    u64::try_from((a as u128).checked_mul(b)?.div_ceil(d)).ok()
}

/// Shares minted for `assets`: `assets × (S + 1) / (A + 1)`, rounded down.
#[inline]
pub fn to_shares(assets: u64, total_assets: u64, total_shares: u64) -> Option<u64> {
    mul_div(assets, total_shares as u128 + 1, total_assets as u128 + 1)
}

/// Shares burned to pay out `assets`: the same ratio, rounded up.
#[inline]
fn to_shares_ceil(assets: u64, total_assets: u64, total_shares: u64) -> Option<u64> {
    mul_div_ceil(assets, total_shares as u128 + 1, total_assets as u128 + 1)
}

/// Assets paid for `shares`: `shares × (A + 1) / (S + 1)`, rounded down.
#[inline]
pub fn to_assets(shares: u64, total_assets: u64, total_shares: u64) -> Option<u64> {
    mul_div(shares, total_assets as u128 + 1, total_shares as u128 + 1)
}

/// How much of `locked` has unlocked `elapsed` seconds into a `window`-second
/// straight line from all of it to none: `locked × elapsed / window`, rounded
/// down, and all of it once the window is over.
#[inline]
pub fn unlocked(locked: u64, elapsed: u64, window: u64) -> u64 {
    if elapsed >= window {
        return locked;
    }
    // elapsed < window, so the quotient is below `locked`.
    (locked as u128 * elapsed as u128 / window as u128) as u64
}

/// The unlock window after `gain` joins `locked`: each amount keeps the time
/// it had left, `time_left` for what was locked and a full `period` for the
/// gain, averaged by size and rounded down. With nothing to lock it is `period`.
#[inline]
pub fn blend(locked: u64, time_left: u64, gain: u64, period: u64) -> u64 {
    let total = locked as u128 + gain as u128;
    if total == 0 {
        return period;
    }
    // A weighted average of two u64s is at most the larger, so it fits.
    ((locked as u128 * time_left as u128 + gain as u128 * period as u128) / total) as u64
}

/// The manager's cut of a gain, in assets, rounded down.
#[inline]
pub fn performance_fee(gain: u64, fee_bps: u16) -> Option<u64> {
    mul_div(gain, fee_bps as u128, BPS)
}

/// The yearly fee on total assets, prorated over `elapsed` seconds, rounded down.
#[inline]
pub fn management_fee(total_assets: u64, fee_bps: u16, elapsed: u64) -> Option<u64> {
    mul_div(
        total_assets,
        (fee_bps as u128).checked_mul(elapsed as u128)?,
        BPS * YEAR,
    )
}

/// Split a gain against losses not yet made up: `(gain beyond them, losses left)`.
/// Only the first part is earned, and only that pays a performance fee.
#[inline]
pub fn recover(gain: u64, loss: u64) -> (u64, u64) {
    let recovered = gain.min(loss);
    (gain - recovered, loss - recovered)
}

/// The fee clock's elapsed time after total assets grow from `before` to
/// `after`: scaled down so the fee accruing on the new total equals what the
/// old total had accrued. Rounded down, in the depositors' favour.
#[inline]
pub fn carry_elapsed(elapsed: u64, before: u64, after: u64) -> u64 {
    if after == 0 {
        return 0;
    }
    // before ≤ after, so the result is at most `elapsed`.
    (elapsed as u128 * before.min(after) as u128 / after as u128) as u64
}

/// Shares that are worth `fee_assets` once minted: `fee × (S + 1) / (A + 1 − fee)`,
/// rounded down. `total_assets` already includes the gain the fee is taken from.
#[inline]
pub fn fee_shares(fee_assets: u64, total_assets: u64, total_shares: u64) -> Option<u64> {
    // A fee of everything, virtual asset included, has no price in shares
    let rest = (total_assets as u128 + 1)
        .checked_sub(fee_assets as u128)
        .filter(|rest| *rest > 0)?;
    mul_div(fee_assets, total_shares as u128 + 1, rest)
}

/// Fulfil a ticket of `shares` against `idle`: `(assets paid, shares burned)`.
/// A ticket that idle covers is paid in full; otherwise idle is paid out and
/// only the shares that payment is worth, rounded up, are burned.
#[inline]
pub fn fulfil(shares: u64, idle: u64, total_assets: u64, total_shares: u64) -> Option<(u64, u64)> {
    let owed = to_assets(shares, total_assets, total_shares)?;
    if owed <= idle {
        return Some((owed, shares));
    }
    let burned = to_shares_ceil(idle, total_assets, total_shares)?;
    Some((idle, burned.min(shares)))
}
