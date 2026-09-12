/// The unresolved `data` + `layer` directives of a scene header.
#[derive(Clone, Debug)]
pub(crate) struct DataRequest {
    pub(crate) manifest: String,
    pub(crate) layer: String,
}
