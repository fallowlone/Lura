mod cli;

use clap::Parser as ClapParser;
use cli::{Cli, Commands};
use folio::{engine, lexer::Lexer, parser, parser::Parser, renderer};
use std::fs;
use std::process;

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
            let input = read_file(&file);
            let mut lexer = Lexer::new(&input);
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);
            let doc = parser.parse().unwrap_or_else(|e| {
                eprintln!("error: {e}");
                process::exit(1);
            });
            let doc = parser::resolver::resolve(doc);
            let doc = parser::id::assign_ids(doc);

            let pdf_bytes = engine::render_pdf(&doc);
            fs::write(&output, &pdf_bytes).unwrap_or_else(|e| {
                eprintln!("error: could not write to {}: {e}", output.display());
                process::exit(1);
            });
            println!("Rendered (Engine v2) → {}", output.display());
        }

        Commands::Convert { file, format, output } => {
            let input = read_file(&file);
            let mut lexer = Lexer::new(&input);
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);
            let doc = parser.parse().unwrap_or_else(|e| {
                eprintln!("error: {e}");
                process::exit(1);
            });
            let doc = parser::resolver::resolve(doc);
            let doc = parser::id::assign_ids(doc);

            if format == "pdf" {
                match renderer::pdf::render(&doc) {
                    Ok(pdf_data) => {
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
                    }
                    Err(e) => {
                        eprintln!("error: failed to generate PDF: {e}");
                        process::exit(1);
                    }
                }
                return;
            }

            let result = match format.as_str() {
                "json" => renderer::json::render(&doc),
                "text" => renderer::text::render(&doc),
                "html" => renderer::html::render(&doc),
                other => {
                    eprintln!("error: unknown format '{other}'. Use json, text, html, or pdf.");
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
    }
}

fn read_file(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: could not read {}: {e}", path.display());
        process::exit(1);
    })
}
