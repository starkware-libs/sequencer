use std::env;

use apollo_infra_utils::path::resolve_project_relative_path;

fn main() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
}
