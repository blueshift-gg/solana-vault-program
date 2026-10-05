use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AdapterError {
    /// The lending account is not the lending program's market for this asset
    InvalidLending,
    /// The fToken account is not the strategy authority's associated token account
    InvalidFTokenAccount,
    /// The lending program account is not the lending program
    InvalidLendingProgram,
    /// A result does not fit in a u64
    MathOverflow,
}

impl From<AdapterError> for ProgramError {
    fn from(e: AdapterError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
