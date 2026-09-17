// TODO(Nadin/Tsabary): reduce visibility throughout this module, and consider unifying with the
// `utils` module.

#[derive(Debug, Clone, Copy)]
pub enum ConfigExpectation {
    Redundant,
    Required,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigPresence {
    Absent,
    Present,
}

impl<T> From<&Option<T>> for ConfigPresence {
    fn from(opt: &Option<T>) -> Self {
        if opt.is_some() { ConfigPresence::Present } else { ConfigPresence::Absent }
    }
}
