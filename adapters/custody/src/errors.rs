use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CustodyError {
    /// An account this program writes directly must be writable
    NotMutable,
    /// Account expected to be a signer
    NotSigner,
    /// The custody account is not this vault's, or is already set up
    InvalidCustody,
    /// The vault account is not a vault using this adapter
    InvalidVault,
    /// The signer is not the vault's owner
    InvalidOwner,
    /// A token account is not the one named at setup, or not fit to be one
    InvalidTokenAccount,
    /// The report is malformed, or not signed by the oracle
    InvalidReport,
    /// The report has expired, or is older than one already accepted
    StaleReport,
    /// The stored price has expired: the oracle must sign a new one
    StalePrice,
    /// A result does not fit in a u64
    MathOverflow,
}

impl From<CustodyError> for ProgramError {
    fn from(e: CustodyError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
