pub const VAULT_VERSION: u8 = 1;
pub const TICKET_VERSION: u8 = 1;

/// Deposits and allocations are blocked; the guardian can resume.
pub const VAULT_PAUSED: u8 = 0;
pub const VAULT_ACTIVE: u8 = 1;
/// Deposits and allocations are blocked for good; every exit still works.
pub const VAULT_WIND_DOWN: u8 = 2;

/// Vault PDA: [VAULT_SEED, share_mint]. Owns the idle and escrow token
/// accounts and is the share mint authority.
pub const VAULT_SEED: &[u8] = b"vault";
/// Strategy authority PDA: [STRATEGY_SEED, vault]. Owns only the strategy
/// token account; it is the one signer an adapter ever receives.
pub const STRATEGY_SEED: &[u8] = b"strategy";
/// Ticket PDA: [TICKET_SEED, vault, owner, id (u64 LE)]. The id is chosen by
/// the client, so concurrent requests never contend for one address.
pub const TICKET_SEED: &[u8] = b"ticket";
/// Event authority PDA: [EVENT_AUTHORITY_SEED]. Every instruction emits one
/// event by invoking the program itself with this PDA as signer, so events
/// live in inner instructions and cannot be spoofed by other programs.
pub const EVENT_AUTHORITY_SEED: &[u8] = b"__event_authority";
// 8dndqb1pgEZKNW4ELci4x4WyNZZZ6HJqwrjaAiooN4Ai; a test re-derives it.
pub const EVENT_AUTHORITY: [u8; 32] = [
    113, 110, 81, 66, 115, 199, 241, 44, 158, 37, 134, 233, 73, 147, 8, 232, 76, 114, 156, 111, 4,
    108, 170, 217, 21, 247, 99, 136, 137, 94, 66, 71,
];
pub const EVENT_AUTHORITY_BUMP: u8 = 254;
/// Self-CPI event instruction discriminator.
pub const EVENT_DISCRIMINATOR: u8 = 255;

pub const CONFIG_LEN: usize = 5 * 32 + 5 * 8 + 2 * 2; // 204
pub const VAULT_LEN: usize = 5 + 10 * 32 + 13 * 8 + 2 * CONFIG_LEN; // 837
pub const TICKET_LEN: usize = 2 + 3 * 32 + 3 * 8; // 122

/// A permit is an authority's Ed25519 signature, made off chain, that lets
/// one subject do one kind of thing in one vault until it expires. Whoever
/// holds it presents it; the authority never has to sign a transaction. A
/// fulfil permit also names the ticket's nonce, a number no other ticket of
/// the vault ever has, so it cannot be used on a later ticket at the same address.
/// The signed message is `permit_message` in this crate.
pub const PERMIT_DOMAIN: &[u8; 16] = b"solana-vault:v1:";
/// Signed by the deposit authority; the subject is the depositor.
pub const PERMIT_DEPOSIT: u8 = 0;
/// Signed by the withdrawal authority; the subject is the ticket.
pub const PERMIT_FULFIL: u8 = 1;
/// A permit on the wire: `[expires_at: i64][signature: [u8; 64]]`.
pub const PERMIT_LEN: usize = 8 + 64;

/// Ceilings from Morpho Vault V2's ConstantsLib (MAX_PERFORMANCE_FEE,
/// MAX_MANAGEMENT_FEE), in basis points per year:
/// https://github.com/morpho-org/vault-v2/blob/main/src/libraries/ConstantsLib.sol
pub const MAX_PERFORMANCE_FEE_BPS: u16 = 5_000;
pub const MAX_MANAGEMENT_FEE_BPS: u16 = 500;
/// Longest a withdrawal authority can keep a ticket to itself, in seconds: the
/// 90 days Drift Vaults allows for its redeem period.
/// https://github.com/drift-labs/drift-vaults
pub const MAX_FULFIL_DELAY: u64 = 90 * 24 * 60 * 60;

/// Adapter interface. Each discriminator is the first eight bytes of
/// `SHA-256("solana-vault-adapter:<name>")`; a test re-derives them.
///
/// Every call carries the same account prefix, then the adapter's own accounts:
///
/// 1. strategy_authority:  [signer on deposit and withdraw]
/// 2. strategy_account:    [mut]   the authority's token account
/// 3. asset_mint:
/// 4. token_program:       [executable]
///
/// - `simulate(data)` sets return data to the position's value as a `u64` LE,
///   not counting the strategy account: the vault adds that balance itself.
/// - `deposit(amount, data)` takes `amount` out of the strategy account.
/// - `withdraw(amount, data)` puts up to `amount` back into it.
///
/// `data` is opaque to the vault. Anyone can call `simulate`, and a fulfilment
/// can call `withdraw`, so an adapter must not trust `data` or its own accounts.
/// The strategy authority is the only signer an adapter ever sees: its own
/// accounts are forwarded with their writable flag and never as signers.
pub const ADAPTER_SIMULATE: [u8; 8] = [118, 54, 75, 38, 122, 8, 79, 163];
pub const ADAPTER_DEPOSIT: [u8; 8] = [75, 202, 153, 250, 190, 191, 210, 61];
pub const ADAPTER_WITHDRAW: [u8; 8] = [249, 216, 188, 15, 65, 135, 104, 39];
/// Accounts one adapter call can carry, prefix included.
pub const MAX_ADAPTER_ACCOUNTS: usize = 32;
/// Opaque bytes one adapter call can carry.
pub const MAX_ADAPTER_DATA: usize = 256;

/// Read interface. Each discriminator is the first eight bytes of
/// `SHA-256("solana-vault-interface:<name>")`; a test re-derives them. Any
/// vault program can implement it, so an integrator prices shares the same
/// way everywhere:
///
/// 1. vault:                               the implementation's vault account
///
/// - `convert_to_shares(assets: u64)`: shares a deposit of `assets` would mint.
/// - `convert_to_assets(shares: u64)`: assets a redemption of `shares` would pay.
///   The two need not be inverses: a vault may price entry above exit.
/// - `max_deposit()`: the largest deposit accepted now.
///
/// Each sets return data to a `u64` LE, readable by CPI or by simulation.
pub const VIEW_CONVERT_TO_SHARES: [u8; 8] = [139, 85, 78, 21, 184, 44, 70, 104];
pub const VIEW_CONVERT_TO_ASSETS: [u8; 8] = [161, 61, 72, 20, 64, 203, 2, 155];
pub const VIEW_MAX_DEPOSIT: [u8; 8] = [93, 242, 13, 103, 162, 85, 83, 106];

// The token programs' account layouts, which SPL Token and Token-2022 share.
// A test packs each account with the token program's own code and reads
// these back.
/// Length of a mint with no extensions.
pub const MINT_LEN: usize = 82;
/// A mint's authority: a four-byte option tag, then the key.
pub const MINT_AUTHORITY: usize = 0;
pub const MINT_DECIMALS: usize = 44;
pub const MINT_IS_INITIALIZED: usize = 45;
/// Length of a token account with no extensions.
pub const TOKEN_ACCOUNT_LEN: usize = 165;
pub const TOKEN_ACCOUNT_OWNER: usize = 32;
pub const TOKEN_ACCOUNT_AMOUNT: usize = 64;
/// Four-byte option tag of the delegate.
pub const TOKEN_ACCOUNT_DELEGATE: usize = 72;
/// 1 is initialized; 0 is not, 2 is frozen.
pub const TOKEN_ACCOUNT_STATE: usize = 108;
/// Four-byte option tag of the close authority.
pub const TOKEN_ACCOUNT_CLOSE_AUTHORITY: usize = 129;
/// Where a Token-2022 account with extensions says what it is, mints being
/// padded to a token account's length, and where its extensions start.
pub const ACCOUNT_TYPE: usize = TOKEN_ACCOUNT_LEN;
pub const ACCOUNT_TYPE_MINT: u8 = 1;
pub const ACCOUNT_TYPE_TOKEN_ACCOUNT: u8 = 2;
pub const EXTENSIONS: usize = ACCOUNT_TYPE + 1;

// TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb; a test checks it against the program.
pub const TOKEN_2022: [u8; 32] = [
    6, 221, 246, 225, 238, 117, 143, 222, 24, 66, 93, 188, 228, 108, 205, 218, 182, 26, 252, 77,
    131, 185, 13, 39, 254, 189, 249, 40, 216, 161, 139, 252,
];
/// The one Token-2022 mint extension an asset may not have active: a transfer
/// hook runs foreign code inside every transfer, needs accounts the vault does
/// not carry, and costs a level of call depth. Every other extension is the
/// manager's call: a fee is measured, and a freeze or a permanent delegate is
/// an issuer power to settle with the issuer.
/// The value is `spl_token_2022_interface::extension::ExtensionType`; a test checks it.
pub const EXTENSION_TRANSFER_HOOK: u16 = 14;
/// The Token-2022 account extension that makes a token account refuse a
/// transfer with no memo. The vault's own accounts must not require one.
pub const EXTENSION_MEMO_TRANSFER: u16 = 8;
