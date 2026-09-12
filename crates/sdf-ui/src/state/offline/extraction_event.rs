use sdf_offline::Progress;

/// Messages forwarded from the extraction thread to the app thread.
pub(crate) enum ExtractionEvent {
    Progress(Progress),
    Finished(Result<(), String>),
}
