use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------- //

use crate::config::sdf::{COAST_ROUTE, MOUNTAIN_ROUTE, RIVER_ROUTE, ROAD_ROUTE};

// ---------------------------------------------------------------------------------------------- //

/// Band kind: the two threshold primitives every recipe is built from.
///
/// `Ramp` — soft edges, open above: `smoothstep(low, high, v)`; coverage
/// rises from 0 at `low` to 1 at `high` and stays at 1 above. The primitive
/// for shorelines, roads, and elevation — anything whose signal is "more =
/// more of the feature".
///
/// `Band` — hard half-open window: `v >= low && v < high`; full ink inside,
/// nothing outside. The primitive for closed windows — water fills, shore
/// outlines, the channel look.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BandKind {
    Ramp,
    Band,
}

/// One styling band: the field's byte range `[low, high)` it paints, the
/// threshold primitive (`kind`), the ink it mixes in, and the strength it
/// mixes at. Every band of every layer carries exactly these fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompositeBand {
    /// Band start in field bytes (0..=255).
    pub low: f32,
    /// Band end in field bytes (0..=255).
    pub high: f32,
    pub kind: BandKind,
    /// The band's color, straight alpha components in 0..=1.
    pub ink: [f32; 3],
    /// Mix strength (0..=1); scales the band's coverage before it paints.
    pub weight: f32,
}

/// One composite layer: an export route plus its styling as bands. List
/// order is paint order, bottom → top; a layer without bands carries no
/// styling and is a configuration error.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompositeLayer {
    /// The export route this layer reads (a `[sdf.tiles]` key or an
    /// `[sdf.export.extra]` name).
    pub name: String,
    /// The styling bands, painted in listed order.
    pub bands: Vec<CompositeBand>,
}

/// The composite view's layer stack in the user config: the array is the
/// source of truth for both which layers compose, their paint order, and —
/// per layer — the styling. Each layer must name an export route and an
/// empty array turns the composite view off (the navbar entry is hidden,
/// not broken).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Composite {
    /// `[[composite.layer]]` — array of tables; renamed so the Rust field
    /// reads `layers`.
    #[serde(default, rename = "layer")]
    pub layers: Vec<CompositeLayer>,
}

impl Default for Composite {
    /// The shipped default palette in paint order (bottom → top): water
    /// fill, shoreline edge, roads, terrain shading, and the wagon-road
    /// route written out like any other. The water fill covers every byte
    /// below the shore line so rivers reach it and lakes do not hollow out;
    /// the hex-grid route is left out of the defaults and re-added by
    /// copying a layer block into the config.
    fn default() -> Self {
        Self {
            layers: vec![
                CompositeLayer {
                    name: String::from(RIVER_ROUTE),
                    bands: vec![
                        CompositeBand {
                            low: 0.0,
                            high: 32.0,
                            kind: BandKind::Band,
                            ink: [0.220, 0.360, 0.450],
                            weight: 1.0,
                        },
                        CompositeBand {
                            low: 32.0,
                            high: 122.0,
                            kind: BandKind::Band,
                            ink: [0.290, 0.450, 0.550],
                            weight: 1.0,
                        },
                    ],
                },
                CompositeLayer {
                    name: String::from(COAST_ROUTE),
                    bands: vec![CompositeBand {
                        low: 122.0,
                        high: 132.0,
                        kind: BandKind::Band,
                        ink: [0.230, 0.330, 0.400],
                        weight: 0.55,
                    }],
                },
                CompositeLayer {
                    name: String::from(ROAD_ROUTE),
                    bands: vec![CompositeBand {
                        low: 116.0,
                        high: 136.0,
                        kind: BandKind::Ramp,
                        ink: [0.420, 0.360, 0.280],
                        weight: 0.8,
                    }],
                },
                CompositeLayer {
                    name: String::from(MOUNTAIN_ROUTE),
                    bands: vec![
                        CompositeBand {
                            low: 96.0,
                            high: 118.0,
                            kind: BandKind::Ramp,
                            ink: [0.480, 0.500, 0.460],
                            weight: 0.12,
                        },
                        CompositeBand {
                            low: 120.0,
                            high: 130.0,
                            kind: BandKind::Ramp,
                            ink: [0.440, 0.460, 0.420],
                            weight: 0.30,
                        },
                    ],
                },
                CompositeLayer {
                    name: String::from(crate::config::export::ROAD_WAGON_ROUTE),
                    bands: vec![CompositeBand {
                        low: 120.0,
                        high: 136.0,
                        kind: BandKind::Band,
                        ink: [0.500, 0.500, 0.500],
                        weight: 0.6,
                    }],
                },
            ],
        }
    }
}
