use drafthouse_cli::{self as cli, Failure};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::run(&args) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(message)) => {
            eprintln!("{message}\n\n{}", cli::USAGE);
            ExitCode::from(2)
        }
        Err(Failure::Domain(error)) => {
            eprintln!(
                "{{\"error\":\"DomainError\",\"message\":{}}}",
                cli::json_string(error.message())
            );
            ExitCode::from(1)
        }
    }
}
