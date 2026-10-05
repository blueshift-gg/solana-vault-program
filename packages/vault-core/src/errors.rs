use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    /// An account this program writes directly must be writable. Accounts
    /// mutated only through a CPI are not checked: the CPI enforces it.
    NotMutable,
    /// Account expected to be a signer
    NotSigner,
    /// Account expected to be owned by our program
    InvalidAccountOwner,
    /// The account data length is not the expected one
    InvalidAccountLength,
    /// The account version is not the expected one
    InvalidVersion,
    /// The account about to be created already exists
    AlreadyInitialized,
    /// The account is not at the address its seeds derive
    InvalidSeeds,
    /// The event CPI was not signed by the event authority
    InvalidEventAuthority,

    /// The signer does not hold the role this instruction requires
    InvalidAuthority,
    /// A fee, rate or timelock is above its ceiling
    InvalidConfig,
    /// No configuration is pending, or its timelock has not passed
    TimelockNotPassed,
    /// The vault status does not allow this instruction
    InvalidStatus,

    /// The mint is not an SPL mint, has a transfer hook, or is not the vault's alone
    InvalidMint,
    /// The token account is not the one the vault recorded, or is not clean
    InvalidTokenAccount,
    /// The adapter accounts do not match the ones the vault recorded
    InvalidAdapter,
    /// The adapter returned no value, a malformed one, or another program's
    InvalidReturnData,
    /// An adapter call carries too many accounts or too much data
    AdapterCallTooLarge,

    /// The last report is older than the vault's maximum age
    StaleReport,
    /// The amount is zero or converts to zero
    ZeroAmount,
    /// The amount is above idle, a cap, or the value it is bounded by
    AmountTooLarge,
    /// Fewer shares would be minted than the depositor accepts
    SlippageExceeded,
    /// The ticket does not belong to this vault, owner or payer
    InvalidTicket,
    /// A permit is required and missing, or its signature is not the authority's
    InvalidPermit,
    /// The permit's expiry has passed
    PermitExpired,
    /// Nothing is idle to pay this ticket with
    NothingToFulfil,
    /// A result does not fit in a u64
    MathOverflow,
    /// The vault's shares are worth nothing right now, so none can be issued
    Worthless,
}

impl From<VaultError> for ProgramError {
    fn from(e: VaultError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
