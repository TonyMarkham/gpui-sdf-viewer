/// Lifecycle of the background extraction pipeline thread.
pub(crate) enum ExtractionStatus {
    Idle,
    Running,
    Failed(String),
    Done,
}
