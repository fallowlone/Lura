use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "folio", about = "Folio document format CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Tokenize a .fol file and print tokens (debug)
    Parse {
        file: PathBuf,
    },
    /// Check a .fol file for syntax errors
    Validate {
        file: PathBuf,
    },
    /// Convert a .fol file to JSON or plain text
    Convert {
        file: PathBuf,
        /// Output format: json, text, html, pdf (default: json)
        #[arg(long, default_value = "json")]
        format: String,
        /// Write output to file instead of stdout
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Render a .fol file to PDF using Engine v2 (taffy layout, unicode line-break)
    Render {
        file: PathBuf,
        /// Output PDF file (default: output.pdf)
        #[arg(long, default_value = "output.pdf")]
        output: PathBuf,
    },
}
