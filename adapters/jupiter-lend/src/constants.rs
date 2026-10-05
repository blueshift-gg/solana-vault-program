use pinocchio::pubkey::Pubkey;

// jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9; a test checks it.
pub const LENDING_PROGRAM: Pubkey = [
    10, 254, 27, 145, 46, 72, 94, 149, 253, 21, 235, 41, 55, 223, 252, 75, 55, 163, 22, 208, 166,
    56, 18, 255, 2, 186, 73, 180, 198, 193, 141, 30,
];

/// Anchor discriminators of the lending program: the first eight bytes of
/// `SHA-256("global:<instruction>")` and `SHA-256("account:Lending")`. A test
/// re-derives them.
pub const LENDING_DEPOSIT: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
pub const LENDING_WITHDRAW: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];
pub const LENDING_ACCOUNT: [u8; 8] = [135, 199, 82, 16, 249, 131, 182, 241];

/// Byte offsets in the lending program's `Lending` account (Borsh, after the
/// discriminator): `mint`, `f_token_mint`, then past `lending_id`, `decimals`,
/// `rewards_rate_model` and `liquidity_exchange_price`, `token_exchange_price`.
/// A test reads a mainnet snapshot at these offsets.
pub const LENDING_MINT: usize = 8;
pub const LENDING_F_TOKEN_MINT: usize = 40;
pub const LENDING_TOKEN_EXCHANGE_PRICE: usize = 115;

/// The liquidity program's `MIN_OPERATE_AMOUNT`: it rejects any smaller
/// supply or withdrawal (`OperateAmountsNearlyZero`).
pub const MIN_OPERATE_AMOUNT: u64 = 10;

/// The lending program's `EXCHANGE_PRICES_PRECISION`.
pub const EXCHANGE_PRICES_PRECISION: u128 = 1_000_000_000_000;
