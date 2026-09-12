/// The staged state of one readback slot's bytes.
pub(crate) enum StagingState {
    /// Unmapped and ready to receive the next frame copy.
    Free,
    /// Copy submitted, waiting for the map callback.
    InFlight,
    /// Map callback fired; bytes are parked until presented.
    Mapped(Vec<u8>),
}
