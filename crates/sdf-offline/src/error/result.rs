use crate::error::OfflineError;

// ---------------------------------------------------------------------------------------------- //

pub type OfflineResult<T> = std::result::Result<T, OfflineError>;
