pub(crate) mod result;

// ---------------------------------------------------------------------------------------------- //

use error_location::ErrorLocation;
use std::panic::Location;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("There was an error: {message} {location}")]
    App {
        message: String,
        location: ErrorLocation,
    },

    #[error("Failed to initialize the UI: {source} {location}")]
    Ui {
        source: Box<sdf_ui::UiError>,
        location: ErrorLocation,
    },
}

impl Error {
    #[track_caller]
    pub fn app(message: &str) -> Self {
        Self::App {
            message: String::from(message),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub fn ui(source: sdf_ui::UiError) -> Self {
        Self::Ui {
            source: Box::new(source),
            location: ErrorLocation::from(Location::caller()),
        }
    }
}
