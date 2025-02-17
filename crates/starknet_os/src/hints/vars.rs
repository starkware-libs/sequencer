#[allow(dead_code)]
pub(crate) enum Scope {
    InitialDict,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
        }
    }
}
