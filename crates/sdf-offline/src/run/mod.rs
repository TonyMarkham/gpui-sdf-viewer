pub(crate) mod extract;
pub(crate) mod field;

// ---------------------------------------------------------------------------------------------- //

pub use crate::run::{extract::run as extract_run, field::run as field_run};

// ---------------------------------------------------------------------------------------------- //

/// Per-file events batch every this many files; step/layer milestones always
/// emit.
pub const PROGRESS_EVERY: usize = 64;

/// One pipeline event: the step (`extract`/`field`/`export`), the layer it
/// concerns, a human-readable message, and whether it is a milestone (always
/// rendered) or a batched per-file counter.
#[derive(Clone, Debug)]
pub struct Progress {
    pub step: &'static str,
    pub layer: String,
    pub message: String,
    pub milestone: bool,
}

impl Progress {
    pub(crate) fn milestone(step: &'static str, message: impl Into<String>) -> Self {
        Self {
            step,
            layer: String::new(),
            message: message.into(),
            milestone: true,
        }
    }

    pub(crate) fn layer(
        step: &'static str,
        layer: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            step,
            layer: layer.into(),
            message: message.into(),
            milestone: true,
        }
    }

    pub(crate) fn batch(
        step: &'static str,
        layer: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            step,
            layer: layer.into(),
            message: message.into(),
            milestone: false,
        }
    }
}
