pub(crate) enum Scope {
    InitialDict,
}

impl From<Scope> for &str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
        }
    }
}
