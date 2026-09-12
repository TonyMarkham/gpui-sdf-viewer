pub mod result;

// ---------------------------------------------------------------------------------------------- //

use error_location::ErrorLocation;
use std::panic::Location;

// ---------------------------------------------------------------------------------------------- //

#[derive(Debug, thiserror::Error)]
pub enum OfflineError {
    #[error("{message} {location}")]
    Io {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Toml {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Paz {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Dds {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Json {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Crypt {
        message: String,
        location: ErrorLocation,
    },

    #[error("{message} {location}")]
    Manifest {
        message: String,
        location: ErrorLocation,
    },
}

impl OfflineError {
    #[track_caller]
    pub fn io(message: impl ToString) -> Self {
        Self::Io {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn toml(message: impl ToString) -> Self {
        Self::Toml {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn paz(message: impl ToString) -> Self {
        Self::Paz {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn dds(message: impl ToString) -> Self {
        Self::Dds {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn json(message: impl ToString) -> Self {
        Self::Json {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn crypt(message: impl ToString) -> Self {
        Self::Crypt {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn manifest(message: impl ToString) -> Self {
        Self::Manifest {
            message: message.to_string(),
            location: ErrorLocation::from(Location::caller()),
        }
    }
}
