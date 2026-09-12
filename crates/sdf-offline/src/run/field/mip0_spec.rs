/// One mip0 tile's expectation in a scaffolded convert scenario.
pub(crate) struct Mip0Spec {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) mip_count: u32,
    pub(crate) payload_len: Option<usize>,
}
