// SPDX-License-Identifier: MulanPSL-2.0

use std::process::ExitCode;
mod acceptance;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "verify-acceptance") {
        let result = (|| {
            if args.len() != 4 {
                return Err("usage: verify-acceptance OUTPUT REPO PI_STATUS".into());
            }
            acceptance::verify(
                std::path::Path::new(&args[1]),
                std::path::Path::new(&args[2]),
                args[3].parse()?,
            )
        })();
        return match result {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        };
    }
    if args.is_empty() || args == ["--help"] {
        println!("{}", rustsbi_dev_build::HELP);
        return ExitCode::SUCCESS;
    }
    if args.len() != 1 {
        eprintln!(
            "Pass the complete invocation as one quoted argument.\n{}",
            rustsbi_dev_build::HELP
        );
        return ExitCode::from(2);
    }
    match rustsbi_dev_build::run(&args[0]) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("rustsbi-dev-build: {error}");
            ExitCode::from(2)
        }
    }
}
