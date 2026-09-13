pub mod composite;
pub mod export;
pub mod extract;
pub mod paths;
pub mod resolved;
pub mod sdf;

// ---------------------------------------------------------------------------------------------- //

pub use crate::config::{
    composite::BandKind, composite::Composite, composite::CompositeBand, composite::CompositeLayer,
    export::Export, extract::Extract, paths::Paths, resolved::ResolvedPaths, sdf::Sdf,
};

use crate::{
    OfflineError, OfflineResult,
    constants::{
        CONFIG_DIR_NAME, CONFIG_FILE_NAME, CONFIG_ROOT_NAME, DATA_DIR_NAME, PAMT_FILE_NAME,
    },
    paz::entry::glob_match,
};

use serde::Deserialize;
use soul_attributes::soul;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, value};

// ---------------------------------------------------------------------------------------------- //

/// The shipped `config.toml` example: what a first run writes when no config
/// exists. Must stay equal to [`Config::defaults`] (asserted by the tests).
pub const DEFAULT_TEMPLATE: &str = r#"# cd-map-offline configuration. Tuning lives here, never in code.
# game_root: the Crimson Desert install directory. Set through the GUI
# picker; contains the pack directory `0012` with its `0.pamt` table.
[paths]
game_root = ""
data_dir  = ""           # optional override; empty = <config dir>/data;
                         # relative values resolve against the config dir

[extract]
packs     = ["0012"]     # the map-screen pack (unencrypted)
includes  = [            # the six worldmap tile families only
    "cd_worldmap_land_sdf*",
    "cd_worldmap_road_sdf*",
    "cd_worldmap_road_wagon_sdf*",
    "cd_worldmap_mountain_sdf*",
    "cd_worldmap_abyss_hex_sdf*",
    "cd_worldmap_blur_height*",
]
dir        = "dds"       # under data_dir: the extracted tiles land here
manifest   = "manifest.json"
dds_header = 128

[sdf.tiles]              # layer -> tile-name prefix (export routes)
coast     = "cd_worldmap_land_sdf"
river     = "cd_worldmap_land_sdf"     # same land field; recipes differ
road      = "cd_worldmap_road_sdf"
mountain  = "cd_worldmap_mountain_sdf"
abyss_hex = "cd_worldmap_abyss_hex_sdf"

[sdf.export]
dir      = "sdf"         # under data_dir: .r8 payloads + the export manifest
manifest = "manifest.json"

[sdf.export.extra]       # fields without a recipe scene, exported under a name
road_wagon  = "cd_worldmap_road_wagon_sdf"
blur_height = "cd_worldmap_blur_height"

[composite]              # the composite view: layers stacked bottom-to-top
# list order = paint order. Every layer: a route name and its bands;
# every band: low, high, kind, ink, weight. kind is "band" (hard window,
# full ink inside) or "ramp" (soft smoothstep rising from low to high,
# staying at full above).

[[composite.layer]]      # water: the deep sea, then the shelf up to the shore
name = "river"
bands = [
    { low = 0.0,   high = 32.0,  kind = "band", ink = [0.220, 0.360, 0.450], weight = 1.0 },
    { low = 32.0,  high = 122.0, kind = "band", ink = [0.290, 0.450, 0.550], weight = 1.0 },
]

[[composite.layer]]      # the shoreline edge: the transition strip only
name = "coast"
bands = [
    { low = 122.0, high = 132.0, kind = "band", ink = [0.230, 0.330, 0.400], weight = 0.55 },
]

[[composite.layer]]
name = "road"
bands = [
    { low = 116.0, high = 136.0, kind = "ramp", ink = [0.420, 0.360, 0.280], weight = 0.8 },
]

[[composite.layer]]
name = "mountain"
bands = [
    { low = 96.0,  high = 118.0, kind = "ramp", ink = [0.480, 0.500, 0.460], weight = 0.12 },
    { low = 120.0, high = 130.0, kind = "ramp", ink = [0.440, 0.460, 0.420], weight = 0.30 },
]

[[composite.layer]]      # a route without an authored recipe: write its band out
name = "road_wagon"
bands = [
    { low = 120.0, high = 136.0, kind = "band", ink = [0.500, 0.500, 0.500], weight = 0.6 },
]
"#;

const PATHS_TABLE: &str = "paths";
const GAME_ROOT_KEY: &str = "game_root";
const DATA_DIR_KEY: &str = "data_dir";

// ---------------------------------------------------------------------------------------------- //

/// The full extraction surface, config-driven: tuning lives here, never in
/// code. A minimal file — just `[paths] game_root` — yields the defaults.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub paths: Paths,
    #[serde(default)]
    pub extract: Extract,
    #[serde(default)]
    pub sdf: Sdf,
    #[serde(default)]
    pub composite: Composite,
}

impl Config {
    /// The single source of the default values; the shipped example and the
    /// tests assert against it.
    pub fn defaults() -> Self {
        Self::default()
    }

    /// Loads the user config from the config home. A missing file yields
    /// [`Config::defaults`]; parse errors surface verbatim with the path.
    pub fn load() -> OfflineResult<Self> {
        Self::load_from(&Self::config_path()?)
    }

    pub fn load_from(path: &Path) -> OfflineResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw)
                .map_err(|e| OfflineError::toml(format!("parse `{}`: {e}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::defaults()),
            Err(error) => Err(OfflineError::io(format!(
                "read `{}`: {error}",
                path.display()
            ))),
        }
    }

    /// Persists the user state (`paths.game_root`/`paths.data_dir`) as a
    /// format-preserving edit: hand-tuned sections and comments survive. When
    /// no file exists yet, the shipped default document is written with the
    /// current values applied. Atomic: temp file + rename.
    pub fn save(&self) -> OfflineResult<()> {
        self.save_to(&Self::config_path()?)
    }

    pub fn save_to(&self, path: &Path) -> OfflineResult<()> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                String::from(DEFAULT_TEMPLATE)
            }
            Err(error) => {
                return Err(OfflineError::io(format!(
                    "read `{}`: {error}",
                    path.display()
                )));
            }
        };
        let mut document = raw
            .parse::<DocumentMut>()
            .map_err(|e| OfflineError::toml(format!("parse `{}`: {e}", path.display())))?;
        document[PATHS_TABLE][GAME_ROOT_KEY] = value(self.paths.game_root.as_str());
        document[PATHS_TABLE][DATA_DIR_KEY] = value(self.paths.data_dir.as_str());

        write_atomic(path, document.to_string())
    }

    /// `<home>/.config/<CONFIG_DIR_NAME>` — the Unix-style config home rooted
    /// at the OS home directory, deliberately not `dirs::config_dir()`.
    pub fn config_dir() -> OfflineResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| OfflineError::io("the OS home directory could not be resolved"))?;
        Ok(home.join(CONFIG_ROOT_NAME).join(CONFIG_DIR_NAME))
    }

    pub fn config_path() -> OfflineResult<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE_NAME))
    }

    /// Resolves every output directory against the config home; no I/O. A
    /// non-empty relative `data_dir` resolves against the config dir, so the
    /// resolved data directory is always absolute.
    pub fn paths(&self) -> OfflineResult<ResolvedPaths> {
        let config_dir = Self::config_dir()?;
        let data_dir = if self.paths.data_dir.is_empty() {
            config_dir.join(DATA_DIR_NAME)
        } else {
            let data_dir = PathBuf::from(&self.paths.data_dir);
            if data_dir.is_absolute() {
                data_dir
            } else {
                config_dir.join(data_dir)
            }
        };
        Ok(ResolvedPaths {
            config_dir,
            dds_dir: data_dir.join(&self.extract.dir),
            sdf_dir: data_dir.join(&self.sdf.export.dir),
            data_dir,
            game_root: PathBuf::from(&self.paths.game_root),
        })
    }

    /// The install is usable when every configured pack carries its `0.pamt`
    /// table; the error names the missing path.
    pub fn validate(&self, game_root: &Path) -> OfflineResult<()> {
        for pack in &self.extract.packs {
            let pamt_path = game_root.join(pack).join(PAMT_FILE_NAME);
            if !pamt_path.is_file() {
                return Err(OfflineError::io(format!(
                    "the Crimson Desert install `{}` is missing `{}`",
                    game_root.display(),
                    pamt_path.display()
                )));
            }
        }

        Ok(())
    }

    /// Hard configuration errors before any I/O: empty includes, a route
    /// prefix no include pattern covers, `[sdf.export.extra]` names colliding
    /// with `[sdf.tiles]` keys, and a composite layer list naming anything
    /// but export routes — each route once.
    pub fn check(&self) -> OfflineResult<()> {
        if self.extract.includes.is_empty() {
            return Err(OfflineError::toml(
                "`[extract] includes` is empty; the pipeline would extract nothing",
            ));
        }

        for (name, prefix) in self.routes() {
            if !self
                .extract
                .includes
                .iter()
                .any(|pattern| glob_match(pattern, &prefix))
            {
                return Err(OfflineError::toml(format!(
                    "export route `{name}` tile prefix `{prefix}` is not covered by any `[extract] includes` pattern"
                )));
            }
        }

        for name in self.sdf.export.extra.keys() {
            if self.sdf.tiles.contains_key(name) {
                return Err(OfflineError::toml(format!(
                    "`[sdf.export.extra]` name `{name}` collides with a `[sdf.tiles]` key"
                )));
            }
        }

        self.check_composite()
    }

    /// The composite layer list: every layer must name an export route and
    /// none may repeat, and every band must carry sane numbers; an empty
    /// layer list is valid (the composite is off).
    #[soul(id = "concept.sdf-scene-contract", step = "composite route validation")]
    fn check_composite(&self) -> OfflineResult<()> {
        for (index, layer) in self.composite.layers.iter().enumerate() {
            if !self.routes().iter().any(|(name, _)| name == &layer.name) {
                return Err(OfflineError::toml(format!(
                    "composite layer \"{}\" is not an export route",
                    layer.name
                )));
            }
            if self.composite.layers[..index]
                .iter()
                .any(|other| other.name == layer.name)
            {
                return Err(OfflineError::toml(format!(
                    "composite layer \"{}\" is listed twice",
                    layer.name
                )));
            }
            if layer.bands.is_empty() {
                return Err(OfflineError::toml(format!(
                    "composite layer \"{}\" carries no bands",
                    layer.name
                )));
            }
            for band in &layer.bands {
                for (name, value) in [("low", band.low), ("high", band.high)] {
                    if !(0.0..=255.0).contains(&value) {
                        return Err(OfflineError::toml(format!(
                            "composite layer \"{}\": band {} {} is outside 0..=255",
                            layer.name, name, value
                        )));
                    }
                }
                if band.low > band.high {
                    return Err(OfflineError::toml(format!(
                        "composite layer \"{}\": band low {} is above high {}",
                        layer.name, band.low, band.high
                    )));
                }
                if !(0.0..=1.0).contains(&band.weight) {
                    return Err(OfflineError::toml(format!(
                        "composite layer \"{}\": band weight {} is outside 0..=1",
                        layer.name, band.weight
                    )));
                }
                for (index, component) in band.ink.iter().enumerate() {
                    if !(0.0..=1.0).contains(component) {
                        return Err(OfflineError::toml(format!(
                            "composite layer \"{}\": band ink component {index} {} is outside 0..=1",
                            layer.name, component
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Every export route: the `[sdf.tiles]` table plus `[sdf.export.extra]`.
    pub fn routes(&self) -> Vec<(String, String)> {
        let mut routes: Vec<(String, String)> = self
            .sdf
            .tiles
            .iter()
            .map(|(name, prefix)| (name.clone(), prefix.clone()))
            .collect();
        routes.extend(
            self.sdf
                .export
                .extra
                .iter()
                .map(|(name, prefix)| (name.clone(), prefix.clone())),
        );
        routes
    }
}

// ---------------------------------------------------------------------------------------------- //

fn write_atomic(path: &Path, contents: String) -> OfflineResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OfflineError::io(format!("create `{}`: {e}", parent.display())))?;
    }

    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, contents)
        .map_err(|e| OfflineError::io(format!("write `{}`: {e}", temp.display())))?;

    std::fs::rename(&temp, path).map_err(|e| {
        OfflineError::io(format!(
            "rename `{}` to `{}`: {e}",
            temp.display(),
            path.display()
        ))
    })
}

// ---------------------------------------------------------------------------------------------- //

#[cfg(test)]
mod tests;
