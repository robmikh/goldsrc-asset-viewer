use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Log debug output to log.txt
    #[clap(long, default_value_t=false)]
    pub log: bool,

    /// Open the specified file
    #[clap(value_parser, value_name = "FILE")]
    pub file_path: Option<PathBuf>,

    /// Export to the given file
    #[clap(value_parser, value_name = "EXPORT FILE")]
    pub export_file_path: Option<PathBuf>,
}
