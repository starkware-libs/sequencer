use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SimpleBootloaderInput {
    pub fact_topologies_path: Option<PathBuf>,
    pub single_page: bool,
    pub tasks: Vec<TaskSpec>,
}

impl SimpleBootloaderInput {
    pub fn from_cairo_pie_path(cairo_pie_path: impl Into<PathBuf>) -> Self {
        Self {
            fact_topologies_path: None,
            single_page: true,
            tasks: vec![TaskSpec::from_cairo_pie_path(cairo_pie_path)],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSpec {
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub path: PathBuf,
    pub program_hash_function: HashFunc,
}

impl TaskSpec {
    pub fn from_cairo_pie_path(path: impl Into<PathBuf>) -> Self {
        Self {
            task_type: TaskType::CairoPiePath,
            path: path.into(),
            program_hash_function: HashFunc::Blake,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum TaskType {
    CairoPiePath,
    RunProgramTask,
    Cairo1Executable,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HashFunc {
    Pedersen,
    Poseidon,
    Blake,
}
