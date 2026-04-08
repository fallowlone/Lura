mod cli;
mod lexer;
mod parser;
mod renderer;

use clap::Parser as ClapParser;
use cli::{Cli, Commands};
use lexer::Lexer;
use parser::Parser;
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
            let _doc = parser.parse();
            println!("✓ valid");
        }

        Commands::Convert { file, format, output } => {
            let input = read_file(&file);
            let mut lexer = Lexer::new(&input);
            let tokens = lexer.tokenize();
            let mut parser = Parser::new(tokens);
            let doc = parser.parse();
            let doc = parser::resolver::resolve(doc);

            let result = match format.as_str() {
                "json" => renderer::json::render(&doc),
                "text" => renderer::text::render(&doc),
                other => {
                    eprintln!("error: unknown format '{other}'. Use json or text.");
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
