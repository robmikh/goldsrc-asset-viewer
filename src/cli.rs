use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Open the specified file
    #[clap(value_parser, value_name = "FILE")]
    pub file_path: Option<PathBuf>,

    /// Export to the given file
    #[clap(value_parser, value_name = "EXPORT FILE")]
    pub export_file_path: Option<PathBuf>,
}
