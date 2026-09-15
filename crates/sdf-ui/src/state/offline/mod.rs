mod extraction_event;
mod extraction_status;
mod offline_status;

pub(crate) use self::extraction_event::ExtractionEvent;
pub(crate) use self::offline_status::OfflineStatus;

use self::extraction_status::ExtractionStatus;
use sdf_offline::{
    Config, OfflineResult, Progress, config::CompositeLayer, extract_run, field_run,
};

// ---------------------------------------------------------------------------------------------- //

use gpui::{AsyncApp, Context, PathPromptOptions, WeakEntity};
use soul_attributes::soul;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

// ---------------------------------------------------------------------------------------------- //

const PICKER_PROMPT: &str = "Select the Crimson Desert install folder";
const DRAIN_TICK: Duration = Duration::from_millis(100);
const PROGRESS_SEPARATOR: &str = " · ";

/// The app config plus its runtime status: whether the game folder is valid,
/// whether extracted data exists, and the extraction progress surface.
pub(crate) struct OfflineState {
    config: Config,
    config_path: Option<PathBuf>,
    game_root_valid: bool,
    data_present: bool,
    rejection: Option<String>,
    load_failure: Option<String>,
    extraction: ExtractionStatus,
    progress_line: String,
}

impl OfflineState {
    pub(crate) fn load() -> Self {
        match Config::load() {
            Ok(config) => Self::from_config(config),
            Err(error) => {
                let mut state = Self::from_config(Config::defaults());
                state.load_failure = Some(error.to_string());
                state
            }
        }
    }

    pub(crate) fn from_config(config: Config) -> Self {
        let mut state = Self {
            config,
            config_path: None,
            game_root_valid: false,
            data_present: false,
            rejection: None,
            load_failure: None,
            extraction: ExtractionStatus::Idle,
            progress_line: String::new(),
        };
        state.refresh();
        state
    }

    /// Test seam: persist saves into this file instead of the user's config
    /// home, so tests never touch real user state.
    #[cfg(test)]
    pub(crate) fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// The resolved data directory — where `config:` scene values point.
    pub(crate) fn data_dir(&self) -> OfflineResult<PathBuf> {
        self.config.paths().map(|paths| paths.data_dir)
    }

    /// The configured composite layer stack (paint order, bottom → top) with
    /// each layer's styled bands; empty when the composite view is off.
    pub(crate) fn composite_layers(&self) -> Vec<CompositeLayer> {
        self.config.composite.layers.clone()
    }

    /// The setup panel is the first-run view: no valid game folder, or no
    /// extracted data yet.
    pub(crate) fn needs_setup(&self) -> bool {
        !self.game_root_valid || !self.data_present
    }

    pub(crate) fn game_root_valid(&self) -> bool {
        self.game_root_valid
    }

    /// The resolved game folder for display; empty when unset.
    pub(crate) fn game_root(&self) -> &str {
        &self.config.paths.game_root
    }

    pub(crate) fn rejection(&self) -> Option<&str> {
        self.rejection.as_deref()
    }

    pub(crate) fn load_failure(&self) -> Option<&str> {
        self.load_failure.as_deref()
    }

    pub(crate) fn extraction_running(&self) -> bool {
        matches!(self.extraction, ExtractionStatus::Running)
    }

    pub(crate) fn extraction_finished(&self) -> bool {
        matches!(
            self.extraction,
            ExtractionStatus::Done | ExtractionStatus::Failed(_)
        )
    }

    pub(crate) fn extraction_failed(&self) -> Option<&str> {
        match &self.extraction {
            ExtractionStatus::Failed(message) => Some(message),
            _ => None,
        }
    }

    /// The one-line extraction progress for the status bar.
    pub(crate) fn progress_line(&self) -> &str {
        &self.progress_line
    }

    /// The whole offline surface in one value: what the status bar renders
    /// and what its tests pin. Passing the struct — not the individual
    /// fields — keeps the renderer's call site from transposing or dropping
    /// arguments.
    pub(crate) fn status(&self) -> OfflineStatus {
        OfflineStatus {
            game_root_valid: self.game_root_valid,
            data_present: self.data_present,
            rejection: self.rejection.clone(),
            game_root: self.config.paths.game_root.clone(),
            extraction_running: self.extraction_running(),
            progress_line: self.progress_line.clone(),
            extraction_failed: self.extraction_failed().map(ToString::to_string),
        }
    }

    /// Opens the platform directory picker and applies the selection.
    pub(crate) fn choose_game_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(PICKER_PROMPT.into()),
        });
        cx.spawn(async move |entity, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = entity.update(cx, |state, cx| state.apply_game_root(path, cx));
        })
        .detach();
    }

    /// Validates the picked folder, saves the config, refreshes the state. A
    /// validation failure stays in the panel with the reason; the dialog
    /// closing with `None` never reaches here.
    pub(crate) fn apply_game_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.rejection = None;
        if let Err(error) = self.config.validate(&path) {
            self.rejection = Some(error.to_string());
            cx.notify();
            return;
        }

        self.config.paths.game_root = path.display().to_string();
        let saved = match &self.config_path {
            Some(config_path) => self.config.save_to(config_path),
            None => self.config.save(),
        };
        if let Err(error) = saved {
            self.rejection = Some(error.to_string());
            cx.notify();
            return;
        }

        self.refresh();
        cx.notify();
    }

    /// Runs the pipeline on a dedicated thread; the app thread drains the
    /// progress channel on a timer tick. No cancel in this slice.
    #[soul(id = "concept.game-data-pipeline", step = "GUI extraction thread")]
    pub(crate) fn start_extraction(&mut self, cx: &mut Context<Self>) {
        if self.extraction_running() {
            return;
        }

        self.extraction = ExtractionStatus::Running;
        self.progress_line.clear();
        let config = self.config.clone();
        let (sender, receiver) = channel::<ExtractionEvent>();
        std::thread::spawn(move || {
            let mut forward = |event: Progress| {
                let _ = sender.send(ExtractionEvent::Progress(event));
            };
            let result =
                extract_run(&config, &mut forward).and_then(|()| field_run(&config, &mut forward));
            let _ = sender.send(ExtractionEvent::Finished(result.map_err(|e| e.to_string())));
        });

        cx.spawn(async move |entity, cx| {
            drain_progress(cx, entity, receiver).await;
        })
        .detach();
        cx.notify();
    }

    fn refresh(&mut self) {
        self.game_root_valid = self.compute_root_validity();
        self.data_present = self.compute_data_present();
    }

    fn compute_root_validity(&self) -> bool {
        if self.config.paths.game_root.is_empty() {
            return false;
        }
        self.config
            .validate(&PathBuf::from(&self.config.paths.game_root))
            .is_ok()
    }

    #[soul(id = "concept.game-data-pipeline", step = "data presence check")]
    fn compute_data_present(&self) -> bool {
        self.config
            .paths()
            .map(|paths| {
                paths
                    .sdf_dir
                    .join(&self.config.sdf.export.manifest)
                    .is_file()
            })
            .unwrap_or(false)
    }

    fn apply_progress(&mut self, cx: &mut Context<Self>, event: Progress) {
        if event.layer.is_empty() {
            self.progress_line = format!("{}{PROGRESS_SEPARATOR}{}", event.step, event.message);
        } else {
            self.progress_line = format!(
                "{}{PROGRESS_SEPARATOR}{}{PROGRESS_SEPARATOR}{}",
                event.step, event.layer, event.message
            );
        }
        cx.notify();
    }

    fn finish_extraction(&mut self, cx: &mut Context<Self>, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.extraction = ExtractionStatus::Done;
                self.progress_line = String::from("extraction complete");
            }
            Err(message) => {
                self.extraction = ExtractionStatus::Failed(message);
            }
        }
        self.refresh();
        cx.notify();
    }
}

// ---------------------------------------------------------------------------------------------- //

/// Timer-ticked drain of the extraction channel on the app thread; entity
/// mutation stays on the app thread. Ends once the outcome is known — either
/// the pipeline finished or the thread died without one.
pub(crate) async fn drain_progress(
    cx: &mut AsyncApp,
    entity: WeakEntity<OfflineState>,
    receiver: Receiver<ExtractionEvent>,
) {
    let mut outcome: Option<Result<(), String>> = None;
    loop {
        let mut events = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(ExtractionEvent::Progress(event)) => events.push(event),
                Ok(ExtractionEvent::Finished(result)) => outcome = Some(result),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    outcome.get_or_insert_with(|| {
                        Err(String::from("the extraction thread ended unexpectedly"))
                    });
                    break;
                }
            }
        }

        let stop = entity
            .update(cx, |state, cx| {
                for event in events {
                    state.apply_progress(cx, event);
                }
                if let Some(result) = outcome.take() {
                    state.finish_extraction(cx, result);
                }
                !state.extraction_running()
            })
            .unwrap_or(true);
        if stop {
            break;
        }

        cx.background_executor().timer(DRAIN_TICK).await;
    }
}
