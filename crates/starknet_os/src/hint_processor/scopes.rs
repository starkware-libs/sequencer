pub enum ScopedVariables {
    Preimage,
}

impl From<ScopedVariables> for &'static str {
    fn from(id: ScopedVariables) -> &'static str {
        match id {
            ScopedVariables::Preimage => "preimage",
        }
    }
}
