use std::io::Write;
use std::process::ExitCode;

use drafthouse_cli::{self as cli, Failure};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let written = match cli::run(&args) {
        Ok(json) => writeln!(std::io::stdout(), "{json}").map(|()| ExitCode::SUCCESS),
        Err(Failure::Usage(message)) => {
            writeln!(std::io::stderr(), "{message}\n\n{}", cli::USAGE).map(|()| ExitCode::from(2))
        }
        Err(Failure::Domain(error)) => writeln!(
            std::io::stderr(),
            "{{\"error\":\"DomainError\",\"message\":{}}}",
            cli::json_string(error.message())
        )
        .map(|()| ExitCode::from(1)),
    };
    written.unwrap_or(ExitCode::FAILURE)
}
