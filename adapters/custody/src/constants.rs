pub const CUSTODY_VERSION: u8 = 1;

/// Custody PDA: [CUSTODY_SEED, strategy_authority]. One per vault.
pub const CUSTODY_SEED: &[u8] = b"custody";
pub const CUSTODY_LEN: usize = 2 + 6 * 32 + 6 * 8; // 242

/// This program's own instructions: the first eight bytes of
/// `SHA-256("solana-vault-custody:<name>")`; a test re-derives them.
pub const INITIALIZE: [u8; 8] = [32, 226, 219, 63, 12, 27, 168, 130];
pub const SET_ORACLE: [u8; 8] = [197, 213, 225, 246, 74, 2, 155, 88];

/// One unit is worth `price / PRICE_SCALE` of the asset. A book starts at par.
pub const PRICE_SCALE: u64 = 1_000_000_000_000;

/// A price report is the oracle's Ed25519 signature, made off chain, over
/// `report_message` in this crate.
pub const REPORT_DOMAIN: &[u8; 16] = b"vault-custody:v1";
/// A price report on the wire: `[price: u64][expires_at: i64][signature: [u8; 64]]`.
pub const REPORT_LEN: usize = 8 + 8 + 64;
