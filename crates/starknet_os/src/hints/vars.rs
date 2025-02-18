pub(crate) enum Scope {
    InitialDict,
    DictTracker,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
            Scope::DictTracker => "dict_tracker",
        }
    }
}

pub(crate) enum Ids {
    BucketIndex,
    DictPtr,
    PrevOffset,
}

impl From<Ids> for &str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::DictPtr => "dict_ptr",
            Ids::BucketIndex => "bucket_index",
            Ids::PrevOffset => "prev_offset",
        }
    }
}
