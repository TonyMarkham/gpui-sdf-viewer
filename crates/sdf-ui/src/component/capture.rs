use std::sync::{Mutex, MutexGuard};

// ---------------------------------------------------------------------------------------------- //

static STATUS_LINE: Mutex<String> = Mutex::new(String::new());
static SETUP_PANEL_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------------------------- //

/// The shell renders into real windows no test reads pixels from; these
/// slots hold the operator-facing text of the last drawn frame so windowed
/// tests can assert the words the operator would read.
pub(crate) fn set_status(line: &str) {
    *lock(&STATUS_LINE) = String::from(line);
}

pub(crate) fn status_line() -> String {
    lock(&STATUS_LINE).clone()
}

pub(crate) fn set_setup_panel(lines: Vec<String>) {
    *lock(&SETUP_PANEL_LINES) = lines;
}

pub(crate) fn setup_panel_lines() -> Vec<String> {
    lock(&SETUP_PANEL_LINES).clone()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
