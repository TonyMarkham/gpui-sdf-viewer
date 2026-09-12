/// The offline status surface, snapshot for rendering: the config's user
/// state plus the runtime extraction status.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OfflineStatus {
    pub(crate) game_root_valid: bool,
    pub(crate) data_present: bool,
    pub(crate) rejection: Option<String>,
    pub(crate) game_root: String,
    pub(crate) extraction_running: bool,
    pub(crate) progress_line: String,
    pub(crate) extraction_failed: Option<String>,
}
