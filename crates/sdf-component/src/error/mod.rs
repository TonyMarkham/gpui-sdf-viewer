pub(crate) mod result;

use error_location::ErrorLocation;
use std::panic::Location;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("No suitable wgpu adapter was found {location}")]
    NoAdapter { location: ErrorLocation },

    #[error("Failed to create the wgpu device: {source} {location}")]
    Device {
        source: wgpu::RequestDeviceError,
        location: ErrorLocation,
    },

    #[error("The SDF scene file could not be read: {source} {location}")]
    SceneFile {
        source: std::io::Error,
        location: ErrorLocation,
    },

    #[error("The SDF scene source is invalid: {message} {location}")]
    SceneInvalid {
        message: String,
        location: ErrorLocation,
    },

    #[error("The SDF scene shader failed to compile: {message} {location}")]
    SceneCompile {
        message: String,
        location: ErrorLocation,
    },

    #[error("The scene data could not be loaded: {message} {location}")]
    Data {
        message: String,
        location: ErrorLocation,
    },

    #[error("Failed to read back the rendered frame: {message} {location}")]
    Readback {
        message: String,
        location: ErrorLocation,
    },
}

impl Error {
    #[track_caller]
    pub(crate) fn no_adapter() -> Self {
        Self::NoAdapter {
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn device(source: wgpu::RequestDeviceError) -> Self {
        Self::Device {
            source,
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn scene_file(source: std::io::Error) -> Self {
        Self::SceneFile {
            source,
            location: ErrorLocation::from(Location::caller()),
        }
    }

    /// Builds the error used when a scene fails pre-GPU validation.
    #[track_caller]
    pub fn scene_invalid(message: &str) -> Self {
        Self::SceneInvalid {
            message: String::from(message),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn scene_compile(message: &str) -> Self {
        Self::SceneCompile {
            message: String::from(message),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    /// Builds the error used when a scene's data (manifest or tile payloads)
    /// fails to load or verify.
    #[track_caller]
    pub fn data(message: &str) -> Self {
        Self::Data {
            message: String::from(message),
            location: ErrorLocation::from(Location::caller()),
        }
    }

    #[track_caller]
    pub(crate) fn readback(message: &str) -> Self {
        Self::Readback {
            message: String::from(message),
            location: ErrorLocation::from(Location::caller()),
        }
    }
}
