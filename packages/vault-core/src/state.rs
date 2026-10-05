//! Account layouts and the accounting steps that mutate them.
//!
//! Every struct has alignment 1: fields are little-endian byte arrays decoded
//! on access, so a struct overlays raw account bytes at any offset, on chain
//! and off. Callers validate owner, length and version before taking views.
//!
//! # Where the price lives
//!
//! A vault's total assets are `idle + debt − locked`, all stored. Deposits,
//! payouts and principal moving to and from the strategy change `idle` and
//! `debt` by the amounts the program itself transferred. Everything else goes
//! through `report`, the one place a claim is adopted: what the adapter says
//! the strategy is worth, and what the idle account holds. A gain it finds is
//! `locked` and reaches the price over the unlock period; a loss reaches it
//! at once.

use crate::constants::*;
use crate::errors::VaultError;
use crate::math;
use pinocchio::{instruction::Seed, program_error::ProgramError, pubkey::Pubkey};

/// Vault signer seeds `[VAULT_SEED, share_mint, bump]`, owned so a handler
/// can build them once and sign with `as_seeds()`.
pub struct VaultSeeds {
    share_mint: Pubkey,
    bump: [u8; 1],
}

impl VaultSeeds {
    #[inline(always)]
    pub fn as_seeds(&self) -> [Seed<'_>; 3] {
        [
            Seed::from(VAULT_SEED),
            Seed::from(&self.share_mint),
            Seed::from(&self.bump),
        ]
    }
}

/// Strategy authority signer seeds `[STRATEGY_SEED, vault, bump]`.
pub struct StrategySeeds {
    vault: Pubkey,
    bump: [u8; 1],
}

impl StrategySeeds {
    #[inline(always)]
    pub fn as_seeds(&self) -> [Seed<'_>; 3] {
        [
            Seed::from(STRATEGY_SEED),
            Seed::from(&self.vault),
            Seed::from(&self.bump),
        ]
    }
}

/// Ticket signer seeds `[TICKET_SEED, vault, owner, id, bump]`.
pub struct TicketSeeds {
    vault: Pubkey,
    owner: Pubkey,
    id: [u8; 8],
    bump: [u8; 1],
}

impl TicketSeeds {
    #[inline(always)]
    pub fn as_seeds(&self) -> [Seed<'_>; 5] {
        [
            Seed::from(TICKET_SEED),
            Seed::from(&self.vault),
            Seed::from(&self.owner),
            Seed::from(&self.id),
            Seed::from(&self.bump),
        ]
    }
}

/// Little-endian getters and setters for a byte-array field.
macro_rules! field {
    ($get:ident, $set:ident, $field:ident, u8) => {
        #[inline(always)]
        pub fn $get(&self) -> u8 {
            self.$field[0]
        }
        #[inline(always)]
        pub fn $set(&mut self, v: u8) {
            self.$field[0] = v;
        }
    };
    ($get:ident, $set:ident, $field:ident, $t:ty) => {
        #[inline(always)]
        pub fn $get(&self) -> $t {
            <$t>::from_le_bytes(self.$field)
        }
        #[inline(always)]
        pub fn $set(&mut self, v: $t) {
            self.$field = v.to_le_bytes();
        }
    };
}

macro_rules! key {
    ($get:ident, $set:ident, $field:ident) => {
        #[inline(always)]
        pub fn $get(&self) -> &Pubkey {
            &self.$field
        }
        #[inline(always)]
        pub fn $set(&mut self, v: Pubkey) {
            self.$field = v;
        }
    };
}

/// Getters only, for a struct that is written whole and never field by field.
macro_rules! read {
    ($field:ident, $t:ty) => {
        #[inline(always)]
        pub fn $field(&self) -> $t {
            <$t>::from_le_bytes(self.$field)
        }
    };
}

macro_rules! read_key {
    ($field:ident) => {
        #[inline(always)]
        pub fn $field(&self) -> &Pubkey {
            &self.$field
        }
    };
}

macro_rules! account {
    ($name:ident) => {
        impl $name {
            /// # Safety
            /// `bytes` must cover the struct's layout; all fields have
            /// alignment 1. The caller validates owner, length and version.
            #[inline(always)]
            pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
                &*(bytes.as_ptr() as *const Self)
            }

            /// # Safety
            /// Same layout requirements as `from_bytes_unchecked`; the caller
            /// must also have exclusive access to the bytes.
            #[inline(always)]
            pub unsafe fn from_bytes_unchecked_mut(bytes: &mut [u8]) -> &mut Self {
                &mut *(bytes.as_mut_ptr() as *mut Self)
            }
        }
    };
}

/// Seconds from `since` to `now`; a clock that went backwards counts as zero.
#[inline(always)]
fn elapsed(now: i64, since: i64) -> u64 {
    now.saturating_sub(since).max(0) as u64
}

/// Everything about a vault that can change after creation. A vault holds it
/// twice: the active one, and one waiting out the timelock. It is only ever
/// replaced whole, so it has no setters. All-zero keys mean "none".
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Config {
    /// Moves funds between idle and the strategy.
    manager: [u8; 32],
    /// Pauses and writes losses off.
    guardian: [u8; 32],
    fee_recipient: [u8; 32],
    /// When set, a deposit needs this key's permit.
    deposit_authority: [u8; 32],
    /// When set, fulfilling a ticket younger than `fulfil_delay` needs this
    /// key's permit.
    withdraw_authority: [u8; 32],
    /// Ceiling on `debt`, checked on allocate.
    debt_cap: [u8; 8],
    /// Ceiling on total assets, checked on deposit.
    deposit_cap: [u8; 8],
    /// Slots a report stays usable for; 0 means the current slot only.
    max_age: [u8; 8],
    /// Seconds after which anyone can fulfil a ticket.
    fulfil_delay: [u8; 8],
    /// Seconds a gain takes to reach the share price once it is reported.
    unlock_period: [u8; 8],
    performance_fee_bps: [u8; 2],
    management_fee_bps: [u8; 2],
}

const _: () = assert!(core::mem::size_of::<Config>() == CONFIG_LEN);

impl Config {
    read_key!(manager);
    read_key!(guardian);
    read_key!(fee_recipient);
    read_key!(deposit_authority);
    read_key!(withdraw_authority);
    read!(debt_cap, u64);
    read!(deposit_cap, u64);
    read!(max_age, u64);
    read!(fulfil_delay, u64);
    read!(unlock_period, u64);
    read!(performance_fee_bps, u16);
    read!(management_fee_bps, u16);

    /// View a configuration in instruction data and check its ceilings.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, ProgramError> {
        if bytes.len() != CONFIG_LEN {
            return Err(ProgramError::InvalidInstructionData);
        }
        // SAFETY: length checked above; all fields have alignment 1.
        let config = unsafe { &*(bytes.as_ptr() as *const Self) };
        if config.performance_fee_bps() > MAX_PERFORMANCE_FEE_BPS
            || config.management_fee_bps() > MAX_MANAGEMENT_FEE_BPS
            || config.fulfil_delay() > MAX_FULFIL_DELAY
        {
            return Err(VaultError::InvalidConfig.into());
        }
        Ok(config)
    }
}

/// One vault: one asset, one share mint, one adapter.
#[repr(C)]
pub struct Vault {
    version: [u8; 1],
    status: [u8; 1],
    bump: [u8; 1],
    strategy_bump: [u8; 1],
    decimals: [u8; 1],
    owner: [u8; 32],
    pending_owner: [u8; 32],
    asset_mint: [u8; 32],
    /// SPL Token or Token-2022, whichever owns the asset mint.
    asset_token_program: [u8; 32],
    share_mint: [u8; 32],
    idle_account: [u8; 32],
    /// Holds the shares of every open ticket.
    escrow_account: [u8; 32],
    /// The vault's one exposure, fixed at creation: the only program deployed
    /// funds are ever handed to.
    adapter: [u8; 32],
    strategy_authority: [u8; 32],
    strategy_account: [u8; 32],
    /// Assets in the idle account, by the program's own count.
    idle: [u8; 8],
    /// Value deployed, as last reported by the adapter or moved as principal.
    debt: [u8; 8],
    /// Shares minted, in escrow, and accrued as fees.
    total_shares: [u8; 8],
    /// Fee shares counted in `total_shares` and not minted yet.
    fee_shares: [u8; 8],
    last_report_slot: [u8; 8],
    /// When gains last unlocked: the start of the line `locked` unlocks along.
    unlock_ts: [u8; 8],
    /// When the management fee was last charged.
    fee_ts: [u8; 8],
    /// Reported losses no later gain has made up yet. Gains pay no
    /// performance fee until they have.
    loss: [u8; 8],
    /// Seconds a submitted configuration waits. Fixed at creation.
    timelock: [u8; 8],
    /// When `pending` becomes executable; 0 means nothing is pending.
    pending_at: [u8; 8],
    /// Gains counted in `idle` and `debt` that the share price does not
    /// include yet. They unlock evenly until `unlock_end`.
    locked: [u8; 8],
    unlock_end: [u8; 8],
    /// Tickets ever opened: the next ticket's nonce.
    tickets: [u8; 8],
    pub config: Config,
    pub pending: Config,
}

const _: () = assert!(core::mem::size_of::<Vault>() == VAULT_LEN);

account!(Vault);

impl Vault {
    field!(version, set_version, version, u8);
    field!(status, set_status, status, u8);
    field!(bump, set_bump, bump, u8);
    field!(strategy_bump, set_strategy_bump, strategy_bump, u8);
    field!(decimals, set_decimals, decimals, u8);
    key!(owner, set_owner, owner);
    key!(pending_owner, set_pending_owner, pending_owner);
    key!(asset_mint, set_asset_mint, asset_mint);
    key!(
        asset_token_program,
        set_asset_token_program,
        asset_token_program
    );
    key!(share_mint, set_share_mint, share_mint);
    key!(idle_account, set_idle_account, idle_account);
    key!(escrow_account, set_escrow_account, escrow_account);
    key!(adapter, set_adapter, adapter);
    key!(
        strategy_authority,
        set_strategy_authority,
        strategy_authority
    );
    key!(strategy_account, set_strategy_account, strategy_account);
    field!(idle, set_idle, idle, u64);
    field!(debt, set_debt, debt, u64);
    field!(total_shares, set_total_shares, total_shares, u64);
    field!(fee_shares, set_fee_shares, fee_shares, u64);
    field!(
        last_report_slot,
        set_last_report_slot,
        last_report_slot,
        u64
    );
    field!(unlock_ts, set_unlock_ts, unlock_ts, i64);
    field!(fee_ts, set_fee_ts, fee_ts, i64);
    field!(loss, set_loss, loss, u64);
    field!(timelock, set_timelock, timelock, u64);
    field!(pending_at, set_pending_at, pending_at, i64);
    field!(locked, set_locked, locked, u64);
    field!(unlock_end, set_unlock_end, unlock_end, i64);
    field!(tickets, set_tickets, tickets, u64);

    #[inline(always)]
    pub fn seeds(share_mint: &Pubkey, bump: u8) -> VaultSeeds {
        VaultSeeds {
            share_mint: *share_mint,
            bump: [bump],
        }
    }

    /// This vault's own signer seeds.
    #[inline(always)]
    pub fn signer_seeds(&self) -> VaultSeeds {
        Self::seeds(self.share_mint(), self.bump())
    }

    #[inline(always)]
    pub fn strategy_seeds(vault: &Pubkey, bump: u8) -> StrategySeeds {
        StrategySeeds {
            vault: *vault,
            bump: [bump],
        }
    }

    /// Everything the vault has counted: idle and debt, gains still locked
    /// included. A deposit is priced against this, a redemption against
    /// `total_assets`. Someone entering while gains are locked pays for them
    /// in full and gets them back as they unlock, so entering takes nothing
    /// from the holders the gains belong to; someone leaving early leaves
    /// their share of what is still locked behind.
    #[inline(always)]
    pub fn counted_assets(&self) -> Result<u64, ProgramError> {
        self.idle()
            .checked_add(self.debt())
            .ok_or(VaultError::MathOverflow.into())
    }

    /// What the shares redeem for together: idle and debt, less the gains
    /// still locked.
    #[inline(always)]
    pub fn total_assets(&self) -> Result<u64, ProgramError> {
        (self.idle() as u128 + self.debt() as u128)
            .checked_sub(self.locked() as u128)
            .and_then(|total| u64::try_from(total).ok())
            .ok_or(VaultError::MathOverflow.into())
    }

    /// The next ticket's nonce.
    #[inline(always)]
    pub fn next_ticket(&mut self) -> Result<u64, ProgramError> {
        let nonce = self.tickets();
        self.set_tickets(nonce.checked_add(1).ok_or(VaultError::MathOverflow)?);
        Ok(nonce)
    }

    /// Whether the stored value may price a deposit or a fulfilment. With no
    /// debt there is nothing a report could change.
    #[inline(always)]
    pub fn is_fresh(&self, slot: u64) -> bool {
        self.debt() == 0 || slot.saturating_sub(self.last_report_slot()) <= self.config.max_age()
    }

    #[inline(always)]
    pub fn check_fresh(&self, slot: u64) -> Result<(), ProgramError> {
        if !self.is_fresh(slot) {
            return Err(VaultError::StaleReport.into());
        }
        Ok(())
    }

    #[inline]
    pub fn to_shares(&self, assets: u64) -> Result<u64, ProgramError> {
        math::to_shares(assets, self.counted_assets()?, self.total_shares())
            .ok_or(VaultError::MathOverflow.into())
    }

    #[inline]
    pub fn to_assets(&self, shares: u64) -> Result<u64, ProgramError> {
        math::to_assets(shares, self.total_assets()?, self.total_shares())
            .ok_or(VaultError::MathOverflow.into())
    }

    /// Room left under the deposit cap; zero unless the vault is active.
    #[inline]
    pub fn max_deposit(&self) -> Result<u64, ProgramError> {
        if self.status() != VAULT_ACTIVE {
            return Ok(0);
        }
        Ok(self
            .config
            .deposit_cap()
            .saturating_sub(self.counted_assets()?))
    }

    /// Mint fee shares worth `fee` assets. They count in `total_shares` at
    /// once and are minted by `CollectFees`.
    fn charge(&mut self, fee: u64) -> Result<(), ProgramError> {
        let total_assets = self.total_assets()?;
        // One charge never takes more than half the vault, however long it
        // went unreported; this also keeps the share formula's divisor large.
        let fee = fee.min(total_assets / 2);
        // A fee is never a reason to stop: if the share count cannot hold
        // all of it, the recipient gets what fits.
        let minted = math::fee_shares(fee, total_assets, self.total_shares())
            .unwrap_or(u64::MAX)
            .min(u64::MAX - self.total_shares());
        self.set_total_shares(self.total_shares() + minted);
        self.set_fee_shares(self.fee_shares() + minted);
        Ok(())
    }

    /// Let locked gains reach the share price for the time that has passed:
    /// they unlock along a straight line from `unlock_ts` to `unlock_end`.
    /// What unlocks is what holders have now earned, so the performance fee
    /// is charged here, on the part beyond earlier losses. The line's start
    /// moves only when something unlocked: an amount that rounds to zero
    /// keeps accruing.
    fn unlock(&mut self, now: i64) -> Result<(), ProgramError> {
        let unlocked = math::unlocked(
            self.locked(),
            elapsed(now, self.unlock_ts()),
            elapsed(self.unlock_end(), self.unlock_ts()),
        );
        if unlocked == 0 {
            return Ok(());
        }
        self.set_locked(self.locked() - unlocked);
        self.set_unlock_ts(now);

        let (earned, loss) = math::recover(unlocked, self.loss());
        self.set_loss(loss);
        let fee = math::performance_fee(earned, self.config.performance_fee_bps())
            .ok_or(VaultError::MathOverflow)?;
        self.charge(fee)
    }

    /// Lock a gain: it joins what is still locked and the whole unlocks
    /// along one line, each part keeping the time it had left.
    fn lock(&mut self, gain: u64, now: i64) -> Result<(), ProgramError> {
        if gain == 0 {
            return Ok(());
        }
        // Both times are kept within the clock's range, so their sum is at
        // most 2^64: what `blend` needs for its weighted sum not to overflow
        // (proved as `blend_fits`).
        let window = math::blend(
            self.locked(),
            elapsed(self.unlock_end(), now),
            gain,
            self.config.unlock_period().min(i64::MAX as u64),
        );
        let locked = self.locked().checked_add(gain);
        // A window too long for the clock ends at the end of time
        let unlock_end = now.saturating_add(i64::try_from(window).unwrap_or(i64::MAX));
        let Some(locked) = locked else {
            return Err(VaultError::MathOverflow.into());
        };
        self.set_locked(locked);
        self.set_unlock_ts(now);
        self.set_unlock_end(unlock_end);
        Ok(())
    }

    /// Take a loss. It comes out of gains still locked first, since those
    /// were never in the price; the rest lowers the price now and is carried
    /// until later gains make it up.
    fn lose(&mut self, lost: u64) {
        let absorbed = lost.min(self.locked());
        self.set_locked(self.locked() - absorbed);
        self.set_loss(self.loss().saturating_add(lost - absorbed));
    }

    /// Bring the vault up to `now`: unlock gains, then charge the management
    /// fee on total assets for the time since the fee clock. Runs before
    /// anything that prices shares or changes total assets, so each holder
    /// earns and pays for the time they held. The fee clock moves only when
    /// a fee was charged: an amount that rounds to zero keeps accruing.
    pub fn accrue(&mut self, now: i64) -> Result<(), ProgramError> {
        self.unlock(now)?;
        let fee = math::management_fee(
            self.total_assets()?,
            self.config.management_fee_bps(),
            elapsed(now, self.fee_ts()),
        )
        .ok_or(VaultError::MathOverflow)?;
        if fee > 0 {
            self.set_fee_ts(now);
            self.charge(fee)?;
        }
        Ok(())
    }

    /// Price a deposit of `assets`, add it to idle and return the shares to
    /// mint. The fee clock is carried so that the fee still accruing stays
    /// what the old total owed: new money never pays for time before it.
    ///
    /// A vault that holds nothing while shares exist takes no deposit: the
    /// first unit in would buy nearly all of whatever comes back later.
    pub fn deposit(&mut self, assets: u64, now: i64) -> Result<u64, ProgramError> {
        self.accrue(now)?;
        let before = self.total_assets()?;
        if self.counted_assets()? == 0 && self.total_shares() > 0 {
            return Err(VaultError::Worthless.into());
        }
        let shares = self.to_shares(assets)?;
        let idle = self.idle().checked_add(assets);
        let total_shares = self.total_shares().checked_add(shares);
        let (Some(idle), Some(total_shares)) = (idle, total_shares) else {
            return Err(VaultError::MathOverflow.into());
        };
        self.set_idle(idle);
        self.set_total_shares(total_shares);

        let carried =
            math::carry_elapsed(elapsed(now, self.fee_ts()), before, self.total_assets()?);
        self.set_fee_ts(now.saturating_sub(carried as i64));
        Ok(shares)
    }

    /// Principal leaves idle for the strategy: `sent` left idle and `received`
    /// arrived, less only by a transfer fee, which is a loss. Callers check
    /// the debt cap.
    #[inline]
    pub fn deploy(&mut self, sent: u64, received: u64) -> Result<(), ProgramError> {
        let idle = self.idle().checked_sub(sent);
        let debt = self.debt().checked_add(received);
        let (Some(idle), Some(debt)) = (idle, debt) else {
            return Err(VaultError::AmountTooLarge.into());
        };
        self.set_idle(idle);
        self.set_debt(debt);
        // What a transfer fee withheld on the way is gone
        self.lose(sent.saturating_sub(received));
        Ok(())
    }

    /// Principal comes back to idle: `sent` left the strategy, at most
    /// `debt`, and `received` arrived, less only by a transfer fee. Total
    /// assets never rise here; a gain is counted by `report` alone.
    #[inline]
    pub fn recall(&mut self, sent: u64, received: u64) -> Result<(), ProgramError> {
        let idle = self.idle().checked_add(received);
        let debt = self.debt().checked_sub(sent);
        let (Some(idle), Some(debt)) = (idle, debt) else {
            return Err(VaultError::AmountTooLarge.into());
        };
        self.set_idle(idle);
        self.set_debt(debt);
        self.lose(sent.saturating_sub(received));
        Ok(())
    }

    /// Redeem `shares` on the spot, if that is possible: the assets to pay,
    /// or `None` when the request has to wait as a ticket. It can be paid now
    /// when the price is fresh, idle covers all of it, and the vault would
    /// not make a new ticket wait for its withdrawal authority.
    pub fn redeem(
        &mut self,
        shares: u64,
        slot: u64,
        now: i64,
    ) -> Result<Option<u64>, ProgramError> {
        let gated = self.config.withdraw_authority() != &[0; 32] && self.config.fulfil_delay() > 0;
        if gated || !self.is_fresh(slot) {
            return Ok(None);
        }
        self.accrue(now)?;
        let assets = self.to_assets(shares)?;
        if assets == 0 || assets > self.idle() {
            return Ok(None);
        }
        let total_shares = self
            .total_shares()
            .checked_sub(shares)
            .ok_or(VaultError::MathOverflow)?;
        self.set_idle(self.idle() - assets);
        self.set_total_shares(total_shares);
        Ok(Some(assets))
    }

    /// Settle a ticket of `shares` against idle: `(assets to pay, shares to
    /// burn)`. Idle pays all of it, or as much as it can and burns only the
    /// shares that payment is worth. Nothing is burned for nothing.
    pub fn fulfil(&mut self, shares: u64, now: i64) -> Result<(u64, u64), ProgramError> {
        self.accrue(now)?;
        let (assets, burned) = math::fulfil(
            shares,
            self.idle(),
            self.total_assets()?,
            self.total_shares(),
        )
        .ok_or(VaultError::MathOverflow)?;
        if assets == 0 {
            return Err(VaultError::NothingToFulfil.into());
        }
        let idle = self.idle().checked_sub(assets);
        let total_shares = self.total_shares().checked_sub(burned);
        let (Some(idle), Some(total_shares)) = (idle, total_shares) else {
            return Err(VaultError::MathOverflow.into());
        };
        self.set_idle(idle);
        self.set_total_shares(total_shares);
        Ok((assets, burned))
    }

    /// Recognise a loss no report will: the strategy is worth `value`, less
    /// than the vault has it at.
    pub fn write_off(&mut self, value: u64, now: i64) -> Result<(), ProgramError> {
        let lost = self
            .debt()
            .checked_sub(value)
            .ok_or(VaultError::AmountTooLarge)?;
        self.accrue(now)?;
        self.set_debt(value);
        self.lose(lost);
        Ok(())
    }

    /// Reprice the vault against two claims: what the adapter says the
    /// strategy is worth, and what the idle account actually holds. This is
    /// the only way total assets change without a deposit or a payout.
    ///
    /// Both are adopted in full. A loss lowers the price at once. A gain,
    /// whether the strategy earned it, someone sent tokens to the idle
    /// account, or funds came back after a write-off, is locked and reaches
    /// the price evenly over the unlock period. So a gain is never a step
    /// that capital arriving just before it, and leaving just after, can take.
    pub fn report(
        &mut self,
        claimed: u64,
        idle_balance: u64,
        now: i64,
        slot: u64,
    ) -> Result<u64, ProgramError> {
        self.accrue(now)?;

        let (idle, debt) = (self.idle(), self.debt());
        let gain = idle_balance
            .saturating_sub(idle)
            .checked_add(claimed.saturating_sub(debt));
        let lost = idle
            .saturating_sub(idle_balance)
            .checked_add(debt.saturating_sub(claimed));
        let (Some(gain), Some(lost)) = (gain, lost) else {
            return Err(VaultError::MathOverflow.into());
        };
        self.set_idle(idle_balance);
        self.set_debt(claimed);
        self.set_last_report_slot(slot);
        self.lock(gain, now)?;
        self.lose(lost);

        // With no unlock period a gain counts straight away
        self.unlock(now)?;
        self.total_assets()?;
        Ok(claimed)
    }
}

/// A redemption request: shares in escrow until fulfilled or cancelled.
#[repr(C)]
pub struct Ticket {
    version: [u8; 1],
    bump: [u8; 1],
    vault: [u8; 32],
    owner: [u8; 32],
    /// Paid the ticket's rent and gets it back when the ticket closes.
    payer: [u8; 32],
    /// Shares still in escrow for this ticket.
    shares: [u8; 8],
    created_at: [u8; 8],
    /// The vault's ticket count when this one was opened. No other ticket of
    /// the vault shares it, even one opened later at the same address.
    nonce: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<Ticket>() == TICKET_LEN);

account!(Ticket);

impl Ticket {
    field!(version, set_version, version, u8);
    field!(bump, set_bump, bump, u8);
    key!(vault, set_vault, vault);
    key!(owner, set_owner, owner);
    key!(payer, set_payer, payer);
    field!(shares, set_shares, shares, u64);
    field!(created_at, set_created_at, created_at, i64);
    field!(nonce, set_nonce, nonce, u64);

    /// Write every field of a freshly created ticket.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn set_inner(
        &mut self,
        bump: u8,
        vault: &Pubkey,
        owner: &Pubkey,
        payer: &Pubkey,
        shares: u64,
        created_at: i64,
        nonce: u64,
    ) {
        self.set_version(TICKET_VERSION);
        self.set_bump(bump);
        self.set_vault(*vault);
        self.set_owner(*owner);
        self.set_payer(*payer);
        self.set_shares(shares);
        self.set_created_at(created_at);
        self.set_nonce(nonce);
    }

    #[inline(always)]
    pub fn seeds(vault: &Pubkey, owner: &Pubkey, id: u64, bump: u8) -> TicketSeeds {
        TicketSeeds {
            vault: *vault,
            owner: *owner,
            id: id.to_le_bytes(),
            bump: [bump],
        }
    }

    /// Whether fulfilling this ticket right now needs the withdrawal authority's permit.
    #[inline(always)]
    pub fn is_gated(&self, config: &Config, now: i64) -> bool {
        config.withdraw_authority() != &[0; 32]
            && elapsed(now, self.created_at()) < config.fulfil_delay()
    }
}
