use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lura", about = "Lura document format CLI")]
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
    /// Convert a .fol file to JSON, text, PDF, or SVG
    Convert {
        file: PathBuf,
        /// Output format: json, text, pdf, svg (default: json)
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
    /// List available printers from the system spooler
    Printers,
    /// Print a .fol file using the system print spooler
    Print {
        file: PathBuf,
        /// Printer name (uses system default when omitted)
        #[arg(long)]
        printer: Option<String>,
        /// Number of copies (default: 1)
        #[arg(long, default_value_t = 1)]
        copies: u32,
        /// Enable duplex mode (two-sided-long-edge)
        #[arg(long, default_value_t = false)]
        duplex: bool,
        /// Page ranges in CUPS format, e.g. "1-3,5"
        #[arg(long)]
        page_ranges: Option<String>,
        /// Generate PDF and print command, but do not send job
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Keep generated temporary PDF file
        #[arg(long, default_value_t = false)]
        keep_pdf: bool,
    },
}
