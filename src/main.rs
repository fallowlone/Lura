mod cli;

use clap::Parser as ClapParser;
use cli::{Cli, Commands};
use lura::{engine, lexer::Lexer, parser, parser::Parser, renderer};
use std::fs;
use std::process;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { file } => {
            let input = read_file(&file);
            let mut lexer = Lexer::new(&input);
            let tokens = lexer.tokenize();
            for token in tokens {
                println!("{:?}", token);
            }
        }

        Commands::Validate { file } => {
            let input = read_file(&file);
            let mut lexer = Lexer::new(&input);
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);
            match parser.parse() {
                Ok(_) => println!("✓ valid"),
                Err(e) => {
                    eprintln!("error: {e}");
                    process::exit(1);
                }
            }
        }

        Commands::Render { file, output } => {
            let doc = load_document(&file);

            let pdf_bytes = engine::render_pdf(&doc);
            fs::write(&output, &pdf_bytes).unwrap_or_else(|e| {
                eprintln!("error: could not write to {}: {e}", output.display());
                process::exit(1);
            });
            println!("Rendered (Engine v2) → {}", output.display());
        }

        Commands::Convert { file, format, output } => {
            let doc = load_document(&file);

            if format == "pdf" {
                let pdf_data = engine::render(
                    &doc,
                    engine::ExportOptions { format: engine::ExportFormat::Pdf },
                );
                match output {
                    Some(path) => {
                        fs::write(&path, &pdf_data).unwrap_or_else(|e| {
                            eprintln!("error: could not write to {}: {e}", path.display());
                            process::exit(1);
                        });
                    }
                    None => {
                        // Default for binary format without output file
                        let def_path = "output.pdf";
                        fs::write(def_path, &pdf_data).unwrap_or_else(|e| {
                            eprintln!("error: could not write to {def_path}: {e}");
                            process::exit(1);
                        });
                        println!("PDF exported successfully to {def_path}");
                    }
                }
                return;
            }

            if format == "svg" {
                let svg_data = String::from_utf8(
                    engine::render(
                        &doc,
                        engine::ExportOptions { format: engine::ExportFormat::Svg },
                    ),
                )
                .unwrap_or_else(|_| String::new());
                match output {
                    Some(path) => {
                        fs::write(&path, svg_data).unwrap_or_else(|e| {
                            eprintln!("error: could not write to {}: {e}", path.display());
                            process::exit(1);
                        });
                    }
                    None => print!("{}", svg_data),
                }
                return;
            }

            let result = match format.as_str() {
                "json" => renderer::json::render(&doc),
                "text" => renderer::text::render(&doc),
                other => {
                    eprintln!("error: unknown format '{other}'. Use json, text, pdf, or svg.");
                    process::exit(1);
                }
            };

            match output {
                Some(path) => {
                    fs::write(&path, &result).unwrap_or_else(|e| {
                        eprintln!("error: could not write to {}: {e}", path.display());
                        process::exit(1);
                    });
                }
                None => print!("{result}"),
            }
        }
        Commands::Printers => {
            list_printers();
        }
        Commands::Print { file, printer, copies, duplex, page_ranges, dry_run, keep_pdf } => {
            let doc = load_document(&file);
            let pdf_data = engine::render(
                &doc,
                engine::ExportOptions { format: engine::ExportFormat::Pdf },
            );

            let temp_pdf = temp_print_pdf_path();
            fs::write(&temp_pdf, pdf_data).unwrap_or_else(|e| {
                eprintln!("error: could not write temp pdf {}: {e}", temp_pdf.display());
                process::exit(1);
            });

            let (tool, args) = build_print_command(
                printer.as_deref(),
                copies,
                duplex,
                page_ranges.as_deref(),
                &temp_pdf,
            );

            if dry_run {
                println!("dry-run: {} {}", tool, shell_join(&args));
                println!("generated: {}", temp_pdf.display());
                if !keep_pdf {
                    let _ = fs::remove_file(&temp_pdf);
                }
                return;
            }

            let output = Command::new(tool).args(&args).output().unwrap_or_else(|e| {
                eprintln!("error: failed to execute print command {}: {e}", tool);
                process::exit(1);
            });
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("error: print failed via {}: {}", tool, stderr.trim());
                process::exit(1);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let response = stdout.trim();
            if response.is_empty() {
                println!("Print job submitted via {}", tool);
            } else {
                println!("Print job submitted via {}: {}", tool, response);
            }

            if keep_pdf {
                println!("Temporary PDF kept at {}", temp_pdf.display());
            } else {
                let _ = fs::remove_file(&temp_pdf);
            }
        }
    }
}

fn read_file(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: could not read {}: {e}", path.display());
        process::exit(1);
    })
}

fn load_document(path: &std::path::Path) -> lura::parser::ast::Document {
    let input = read_file(path);
    let mut lexer = Lexer::new(&input);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let doc = parser.parse().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    });
    let doc = parser::resolver::resolve(doc);
    parser::id::assign_ids(doc)
}

fn temp_print_pdf_path() -> std::path::PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("lura-print-{}-{}.pdf", process::id(), ts))
}

fn build_print_command(
    printer: Option<&str>,
    copies: u32,
    duplex: bool,
    page_ranges: Option<&str>,
    file: &std::path::Path,
) -> (&'static str, Vec<String>) {
    if command_exists("lp") {
        let mut args = Vec::new();
        if let Some(name) = printer {
            args.push("-d".to_string());
            args.push(name.to_string());
        }
        if copies > 1 {
            args.push("-n".to_string());
            args.push(copies.to_string());
        }
        if duplex {
            args.push("-o".to_string());
            args.push("sides=two-sided-long-edge".to_string());
        }
        if let Some(ranges) = page_ranges {
            args.push("-P".to_string());
            args.push(ranges.to_string());
        }
        args.push(file.display().to_string());
        ("lp", args)
    } else if command_exists("lpr") {
        let mut args = Vec::new();
        if let Some(name) = printer {
            args.push("-P".to_string());
            args.push(name.to_string());
        }
        if copies > 1 {
            args.push("-#".to_string());
            args.push(copies.to_string());
        }
        if duplex {
            args.push("-o".to_string());
            args.push("sides=two-sided-long-edge".to_string());
        }
        if let Some(ranges) = page_ranges {
            args.push("-o".to_string());
            args.push(format!("page-ranges={ranges}"));
        }
        args.push(file.display().to_string());
        ("lpr", args)
    } else {
        eprintln!("error: no print command found (expected lp or lpr in PATH)");
        process::exit(1);
    }
}

fn list_printers() {
    if command_exists("lpstat") {
        let output = Command::new("lpstat")
            .args(["-p", "-d"])
            .output()
            .unwrap_or_else(|e| {
                eprintln!("error: failed to run lpstat: {e}");
                process::exit(1);
            });
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("error: lpstat failed: {}", stderr.trim());
            process::exit(1);
        }
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return;
    }

    if command_exists("lp") {
        let output = Command::new("lp").arg("-d").output().unwrap_or_else(|e| {
            eprintln!("error: failed to run lp -d: {e}");
            process::exit(1);
        });
        if output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            return;
        }
    }

    eprintln!("error: could not list printers (lpstat/lp unavailable)");
    process::exit(1);
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
        || Command::new("which").arg(command).output().map(|o| o.status.success()).unwrap_or(false)
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.chars().all(|c| c.is_ascii_alphanumeric() || "/._-=:".contains(c)) {
                arg.clone()
            } else {
                format!("\"{}\"", arg.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
