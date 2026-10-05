//! The custody account's layout, the book-keeping steps that mutate it, and
//! the unit arithmetic. Every `unsafe` needed to overlay the layout on
//! account bytes lives here; handlers only see checked views.

use crate::{constants::*, errors::CustodyError};
use pinocchio::{
    account_info::AccountInfo, instruction::Seed, program_error::ProgramError, pubkey::Pubkey,
};

/// The message a price report signs: the domain, this program, the strategy
/// authority it speaks for, the book it prices, the price and the expiry as a
/// Unix timestamp.
pub fn report_message(
    strategy_authority: &[u8; 32],
    book: u64,
    price: u64,
    expires_at: i64,
) -> [u8; 104] {
    let mut message = [0u8; 104];
    message[..16].copy_from_slice(REPORT_DOMAIN);
    message[16..48].copy_from_slice(&crate::ID);
    message[48..80].copy_from_slice(strategy_authority);
    message[80..88].copy_from_slice(&book.to_le_bytes());
    message[88..96].copy_from_slice(&price.to_le_bytes());
    message[96..].copy_from_slice(&expires_at.to_le_bytes());
    message
}

/// Units bought by `amount` at `price`, rounded down.
#[inline]
pub fn to_units(amount: u64, price: u64) -> Option<u64> {
    u64::try_from(amount as u128 * PRICE_SCALE as u128 / price.max(1) as u128).ok()
}

/// Units redeemed by `amount` coming back at `price`, rounded up.
#[inline]
pub fn to_units_ceil(amount: u64, price: u64) -> Option<u64> {
    u64::try_from((amount as u128 * PRICE_SCALE as u128).div_ceil(price.max(1) as u128)).ok()
}

/// What `units` are worth at `price`, rounded down.
#[inline]
pub fn to_value(units: u64, price: u64) -> Option<u64> {
    u64::try_from(units as u128 * price as u128 / PRICE_SCALE as u128).ok()
}

/// Custody signer seeds `[CUSTODY_SEED, strategy_authority, bump]`.
pub struct CustodySeeds {
    strategy_authority: Pubkey,
    bump: [u8; 1],
}

impl CustodySeeds {
    #[inline(always)]
    pub fn as_seeds(&self) -> [Seed<'_>; 3] {
        [
            Seed::from(CUSTODY_SEED),
            Seed::from(&self.strategy_authority),
            Seed::from(&self.bump),
        ]
    }
}

/// One vault's off-chain book. Alignment 1, so it overlays account bytes.
#[repr(C)]
pub struct Custody {
    version: [u8; 1],
    bump: [u8; 1],
    strategy_authority: [u8; 32],
    vault: [u8; 32],
    /// The only token account `Deposit` sends to. Fixed at setup.
    destination: [u8; 32],
    /// The token account funds come back to, owned by this custody. Fixed at setup.
    return_account: [u8; 32],
    /// Signs price reports.
    oracle: [u8; 32],
    /// Replaces `oracle` once `pending_oracle_at` has passed; zero is none.
    pending_oracle: [u8; 32],
    pending_oracle_at: [u8; 8],
    /// Units of the off-chain position this vault holds.
    units: [u8; 8],
    /// Price of one unit, scaled by `PRICE_SCALE`, as last reported.
    price: [u8; 8],
    /// When `price` stops being usable. Also the floor for the next report's
    /// expiry, so a report cannot be replayed once a later one was accepted.
    price_expires_at: [u8; 8],
    /// How much of the return account's balance has been settled against
    /// units. The rest arrived since and still counts as units.
    settled: [u8; 8],
    /// Counts the books this custody has held. A book starts when funds go
    /// out with no units outstanding, always at par. A price report names
    /// its book, so one signed for an earlier book cannot price this one.
    book: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<Custody>() == CUSTODY_LEN);

impl Custody {
    /// View a custody from its raw bytes, off chain. The caller knows the
    /// account is this program's.
    #[inline(always)]
    pub fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        // SAFETY: length checked; all fields have alignment 1.
        (bytes.len() == CUSTODY_LEN).then(|| unsafe { &*(bytes.as_ptr() as *const Self) })
    }

    /// View the account after checking owner, length and version. The
    /// program never holds a checked borrow, so callers must not alias a
    /// mutable view of the same account.
    #[inline(always)]
    pub fn load(account: &AccountInfo) -> Result<&Self, ProgramError> {
        if !account.is_owned_by(&crate::ID) || account.data_len() != CUSTODY_LEN {
            return Err(CustodyError::InvalidCustody.into());
        }
        // SAFETY: length checked above; all fields have alignment 1.
        let this = unsafe { &*(account.borrow_data_unchecked().as_ptr() as *const Self) };
        if this.version[0] != CUSTODY_VERSION {
            return Err(CustodyError::InvalidCustody.into());
        }
        Ok(this)
    }

    /// Same checks as `load`, returning a mutable view.
    #[allow(clippy::mut_from_ref)]
    #[inline(always)]
    pub fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        Self::load(account)?;
        if !account.is_writable() {
            return Err(CustodyError::NotMutable.into());
        }
        // SAFETY: same layout guarantees as `load`; the caller holds no other view.
        Ok(unsafe { &mut *(account.borrow_mut_data_unchecked().as_mut_ptr() as *mut Self) })
    }

    /// View an account this instruction just created: program-owned, exactly
    /// sized and still zeroed, so `set_inner` is the only valid next step.
    #[allow(clippy::mut_from_ref)]
    #[inline(always)]
    pub fn load_new(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        if !account.is_owned_by(&crate::ID) || account.data_len() != CUSTODY_LEN {
            return Err(CustodyError::InvalidCustody.into());
        }
        // SAFETY: length checked above; all fields have alignment 1.
        let this = unsafe { &mut *(account.borrow_mut_data_unchecked().as_mut_ptr() as *mut Self) };
        if this.version[0] != 0 {
            return Err(CustodyError::InvalidCustody.into());
        }
        Ok(this)
    }

    /// Write every field of a freshly created custody: an empty book at par.
    #[inline(always)]
    pub fn set_inner(
        &mut self,
        bump: u8,
        strategy_authority: &Pubkey,
        vault: &Pubkey,
        destination: &Pubkey,
        return_account: &Pubkey,
        oracle: &Pubkey,
    ) {
        self.version = [CUSTODY_VERSION];
        self.bump = [bump];
        self.strategy_authority = *strategy_authority;
        self.vault = *vault;
        self.destination = *destination;
        self.return_account = *return_account;
        self.oracle = *oracle;
        self.price = PRICE_SCALE.to_le_bytes();
    }

    #[inline(always)]
    pub fn strategy_authority(&self) -> &Pubkey {
        &self.strategy_authority
    }

    #[inline(always)]
    pub fn vault(&self) -> &Pubkey {
        &self.vault
    }

    #[inline(always)]
    pub fn destination(&self) -> &Pubkey {
        &self.destination
    }

    #[inline(always)]
    pub fn return_account(&self) -> &Pubkey {
        &self.return_account
    }

    #[inline(always)]
    pub fn oracle(&self) -> &Pubkey {
        &self.oracle
    }

    #[inline(always)]
    pub fn units(&self) -> u64 {
        u64::from_le_bytes(self.units)
    }

    #[inline(always)]
    pub fn price(&self) -> u64 {
        u64::from_le_bytes(self.price)
    }

    #[inline(always)]
    pub fn price_expires_at(&self) -> i64 {
        i64::from_le_bytes(self.price_expires_at)
    }

    #[inline(always)]
    pub fn settled(&self) -> u64 {
        u64::from_le_bytes(self.settled)
    }

    #[inline(always)]
    pub fn book(&self) -> u64 {
        u64::from_le_bytes(self.book)
    }

    /// This custody's own signer seeds.
    #[inline(always)]
    pub fn signer_seeds(&self) -> CustodySeeds {
        CustodySeeds {
            strategy_authority: self.strategy_authority,
            bump: self.bump,
        }
    }

    /// Name the oracle that takes over at `takes_over`.
    #[inline(always)]
    pub fn set_pending_oracle(&mut self, oracle: &Pubkey, takes_over: i64) {
        self.pending_oracle = *oracle;
        // Zero means none pending, so a wait that lands on zero is nudged off it
        self.pending_oracle_at = takes_over.max(1).to_le_bytes();
    }

    /// Let a pending oracle take over once its wait is done.
    #[inline(always)]
    pub fn promote_oracle(&mut self, now: i64) {
        let pending_at = i64::from_le_bytes(self.pending_oracle_at);
        if pending_at != 0 && now >= pending_at {
            self.oracle = self.pending_oracle;
            self.pending_oracle = [0; 32];
            self.pending_oracle_at = [0; 8];
        }
    }

    /// Store a price the oracle signed. Callers check the signature, and that
    /// the report is neither expired nor older than the stored one.
    #[inline(always)]
    pub fn set_price(&mut self, price: u64, expires_at: i64) {
        self.price = price.to_le_bytes();
        self.price_expires_at = expires_at.to_le_bytes();
    }

    /// The price to trade units at. An empty book restarts at par; otherwise
    /// the stored price must not have expired.
    #[inline]
    pub fn live_price(&mut self, now: i64) -> Result<u64, ProgramError> {
        if self.units() == 0 {
            self.price = PRICE_SCALE.to_le_bytes();
            return Ok(PRICE_SCALE);
        }
        if now > self.price_expires_at() {
            return Err(CustodyError::StalePrice.into());
        }
        Ok(self.price())
    }

    /// What the book is worth at `price`: its units, plus returned funds
    /// already settled. Funds that arrived since are still counted as the
    /// units they will redeem.
    #[inline]
    pub fn value(&self, price: u64) -> Result<u64, ProgramError> {
        to_value(self.units(), price)
            .and_then(|value| value.checked_add(self.settled()))
            .ok_or(CustodyError::MathOverflow.into())
    }

    /// `amount` went out to custody: it buys units at `price`. With no units
    /// outstanding this opens a new book.
    #[inline]
    pub fn buy(&mut self, amount: u64, price: u64) -> Result<(), ProgramError> {
        if self.units() == 0 {
            self.book = self.book().wrapping_add(1).to_le_bytes();
        }
        let units = to_units(amount, price)
            .and_then(|bought| self.units().checked_add(bought))
            .ok_or(CustodyError::MathOverflow)?;
        self.units = units.to_le_bytes();
        Ok(())
    }

    /// The return account holds `held`. Settle what arrived since last time
    /// against units at `price`, and release up to `amount` of it to the
    /// vault: the amount to transfer. More coming back than the book holds
    /// empties the book; the rest is a gain for the vault to count.
    #[inline]
    pub fn redeem(&mut self, held: u64, amount: u64, price: u64) -> Result<u64, ProgramError> {
        let arrived = held.saturating_sub(self.settled());
        // More units than a u64 holds is more than the book has: all of them
        let redeemed = to_units_ceil(arrived, price).unwrap_or(u64::MAX);
        self.units = self.units().saturating_sub(redeemed).to_le_bytes();

        let amount = amount.min(held);
        self.settled = (held - amount).to_le_bytes();
        Ok(amount)
    }
}
