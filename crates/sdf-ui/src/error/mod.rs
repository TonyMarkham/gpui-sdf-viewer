pub(crate) mod result;

// ---------------------------------------------------------------------------------------------- //

use error_location::ErrorLocation;
use std::panic::Location;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to load the UI theme: {source} {location}")]
    Theme {
        source: serde_json::Error,
        location: ErrorLocation,
    },

    #[error("The embedded UI theme contains no themes {location}")]
    ThemeMissing { location: ErrorLocation },

    #[error("Failed to load the scene: {source} {location}")]
    Scene {
        source: sdf_component::SdfError,
        location: ErrorLocation,
    },
}

impl Error {
    #[track_caller]
    pub(crate) fn theme(source: serde_json::Error) -> Self {
        Self::Theme {
            source,
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn theme_missing() -> Self {
        Self::ThemeMissing {
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn scene(source: sdf_component::SdfError) -> Self {
        Self::Scene {
            source,
            location: ErrorLocation::from(Location::caller()),
        }
    }
}
