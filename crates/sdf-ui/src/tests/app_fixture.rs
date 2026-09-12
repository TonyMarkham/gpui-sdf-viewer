use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use super::{
    TEST_INSTALL_DIR, TEST_PACK, TEST_PAMT, TEST_SCENES_DIR, TEST_WORK_DIR, ensure_dir, write_bytes,
};
use crate::state::offline::OfflineState;
use crate::{RootView, state::app::App};
use std::path::PathBuf;

use super::app_host::AppHost;

/// A scratch tree over a stub install: a pack with its pamt table, a scenes
/// directory, and a work directory with an export manifest placeholder.
pub(crate) struct AppFixture {
    pub(crate) root: PathBuf,
    pub(crate) scenes: PathBuf,
}

impl AppFixture {
    /// The suite's only windowed constructor: one window over an explicit
    /// offline state. Kept single on purpose — several windows owned
    /// concurrently dead-lock the gpui test platform, so every windowed
    /// assertion lives in the one flow test below.
    pub(crate) fn app_with<'a>(
        &self,
        offline: OfflineState,
        cx: &'a mut TestAppContext,
    ) -> (Entity<App>, &'a mut VisualTestContext) {
        cx.update(|cx| assert!(crate::initialize(cx).is_ok()));
        let scenes = self.scenes.clone();
        let app_state = cx.new(move |cx| {
            let entity = cx.new(|_| offline);
            App::build(entity, Some(&scenes), cx)
        });
        let app_state_clone = app_state.clone();
        let (_host, cx) = cx.add_window_view(move |_, cx| {
            let root_view = cx.new(|cx| RootView::new(app_state_clone, cx));
            AppHost { root: root_view }
        });

        (app_state, cx)
    }

    /// A windowless app state over an explicit offline state. Windowless on
    /// purpose: tests that assert state only must not own windows, so the
    /// suite keeps exactly one windowed test and no runner can ever own
    /// several windows concurrently.
    pub(crate) fn app_state_with_offline(
        &self,
        offline: OfflineState,
        cx: &mut TestAppContext,
    ) -> Entity<App> {
        cx.update(|cx| assert!(crate::initialize(cx).is_ok()));
        let scenes = self.scenes.clone();
        cx.new(move |cx| {
            let entity = cx.new(|_| offline);
            App::build(entity, Some(&scenes), cx)
        })
    }

    pub(crate) fn offline(&self) -> OfflineState {
        let mut config = sdf_offline::Config::defaults();
        config.paths.game_root = self.root.join(TEST_INSTALL_DIR).display().to_string();
        config.paths.data_dir = self.root.join(TEST_WORK_DIR).display().to_string();
        OfflineState::from_config(config)
    }
}

impl Drop for AppFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Scaffolds the scratch tree for `tag`: a stub install pack, a scenes
/// directory, and a work directory with a manifest placeholder.
pub(crate) fn fixture(tag: &str, _cx: &mut TestAppContext) -> AppFixture {
    let root = std::env::temp_dir().join(format!("sdf-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let install = root.join(TEST_INSTALL_DIR);
    let scenes = root.join(TEST_SCENES_DIR);
    ensure_dir(&install.join(TEST_PACK));
    write_bytes(&install.join(TEST_PACK).join(TEST_PAMT), b"pamt");
    ensure_dir(&scenes);
    let work = root.join(TEST_WORK_DIR);
    ensure_dir(&work.join("sdf"));
    write_bytes(&work.join("sdf").join("manifest.json"), b"{}");
    AppFixture { root, scenes }
}
