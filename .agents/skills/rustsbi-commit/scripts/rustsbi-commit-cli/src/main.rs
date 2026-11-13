// SPDX-License-Identifier: MulanPSL-2.0

use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 3 || args[0] != "check" {
        eprintln!("Usage: rustsbi-commit-cli check REQUEST.json NEW_REPORT.json");
        return ExitCode::from(2);
    }
    match rustsbi_commit_cli::check(Path::new(&args[1]), Path::new(&args[2])) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::from(2)
        }
    }
}
