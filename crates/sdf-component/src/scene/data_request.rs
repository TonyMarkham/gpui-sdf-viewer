/// The unresolved `data` + `layer`/`layers` directives of a scene header.
/// `layers` carries the composite form — the comma-separated layer list in
/// paint order, bottom → top; a one-entry list is the single-field `layer`
/// form.
#[derive(Clone, Debug)]
pub(crate) struct DataRequest {
    pub(crate) manifest: String,
    pub(crate) layers: Vec<String>,
}
