/// The scheme of a scene header's `data` value.
pub(crate) enum Scheme {
    Plain,
    Drive,
    Config(String),
    Unknown(String),
}

pub(crate) fn scheme(value: &str) -> Scheme {
    let Some((head, remainder)) = value.split_once(':') else {
        return Scheme::Plain;
    };
    if head.len() == 1 {
        return Scheme::Drive;
    }
    if head.eq_ignore_ascii_case(super::CONFIG_SCHEME) {
        return Scheme::Config(String::from(remainder));
    }
    Scheme::Unknown(String::from(head))
}
