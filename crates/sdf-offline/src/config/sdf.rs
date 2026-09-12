use crate::config::export::Export;

// ---------------------------------------------------------------------------------------------- //

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------- //

pub(crate) const COAST_ROUTE: &str = "coast";
pub(crate) const RIVER_ROUTE: &str = "river";
pub(crate) const ROAD_ROUTE: &str = "road";
pub(crate) const MOUNTAIN_ROUTE: &str = "mountain";
pub(crate) const ABYSS_HEX_ROUTE: &str = "abyss_hex";

// ---------------------------------------------------------------------------------------------- //

/// The export route table: layer name → tile-name prefix as recorded in the
/// extract manifest.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Sdf {
    #[serde(default)]
    pub tiles: BTreeMap<String, String>,
    #[serde(default)]
    pub export: Export,
}

impl Default for Sdf {
    fn default() -> Self {
        let mut tiles = BTreeMap::new();
        tiles.insert(
            String::from(COAST_ROUTE),
            String::from(crate::config::export::LAND_PREFIX),
        );
        tiles.insert(
            String::from(RIVER_ROUTE),
            String::from(crate::config::export::LAND_PREFIX),
        );
        tiles.insert(
            String::from(ROAD_ROUTE),
            String::from(crate::config::export::ROAD_PREFIX),
        );
        tiles.insert(
            String::from(MOUNTAIN_ROUTE),
            String::from(crate::config::export::MOUNTAIN_PREFIX),
        );
        tiles.insert(
            String::from(ABYSS_HEX_ROUTE),
            String::from(crate::config::export::ABYSS_HEX_PREFIX),
        );

        Self {
            tiles,
            export: Export::default(),
        }
    }
}
