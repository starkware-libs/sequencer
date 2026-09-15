use std::path::PathBuf;
use std::process::Command;

const LIST_NAME_FLAG: &str = "--allowed-libfuncs-list-name";
const LIST_FILE_FLAG: &str = "--allowed-libfuncs-list-file";

/// The allowed-libfuncs list to pass to a Cairo compiler binary: either one of its built-in lists,
/// or a list file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibfuncArg {
    ListName(String),
    ListFile(PathBuf),
}

impl LibfuncArg {
    pub fn add_to_command<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        match self {
            Self::ListName(name) => command.arg(LIST_NAME_FLAG).arg(name),
            Self::ListFile(path) => command.arg(LIST_FILE_FLAG).arg(path),
        }
    }

    /// The two CLI tokens this argument expands to, or `None` for a non-UTF-8 list path.
    pub fn to_cli_args(&self) -> Option<[String; 2]> {
        match self {
            Self::ListName(name) => Some([LIST_NAME_FLAG.to_string(), name.clone()]),
            Self::ListFile(path) => Some([LIST_FILE_FLAG.to_string(), path.to_str()?.to_string()]),
        }
    }
}
