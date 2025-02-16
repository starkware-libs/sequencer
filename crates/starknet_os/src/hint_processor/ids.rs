pub enum Ids {
    InitialRoot,
    FinalRoot,
}

impl From<Ids> for &'static str {
    fn from(id: Ids) -> &'static str {
        match id {
            Ids::InitialRoot => "initial_root",
            Ids::FinalRoot => "final_root",
        }
    }
}
