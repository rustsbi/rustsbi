//! rustsbi-review-cli — the whole Rust script layer of the rustsbi-code-review
//! skill: deterministic checks plus LLM review orchestration.
//!
//! Subcommands (one per former Python script):
//!   checks          deterministic check pipeline        (checks/cli.py)
//!   resolve-target  build the canonical review input    (resolve_target.py)
//!   run-pass        spawn one review pass               (run_pass.py)
//!   run-fanout      combined or persona fan-out         (run_fanout.py)
//!   assemble        merge comments into the review file (assemble_review.py)
//!   archive         snapshot a run into a stamp dir     (archive_run.py)
//!   usage           token accounting report             (usage.py)

mod archive;
mod assemble;
mod build_stage;
mod checks;
mod convention;
mod fanout;
mod gitutil;
mod lint;
mod model;
mod pass;
mod report;
mod resolve;
mod scope;
mod skillenv;
mod test_stage;
mod usage_report;
mod util;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match argv.first().map(String::as_str) {
        Some("checks") => checks::checks_main(&argv[1..]),
        Some("resolve-target") => resolve::resolve_main(&argv[1..]),
        Some("run-pass") => pass::pass_main(&argv[1..]),
        Some("run-fanout") => fanout::fanout_main(&argv[1..]),
        Some("assemble") => assemble::assemble_main(&argv[1..]),
        Some("archive") => archive::archive_main(&argv[1..]),
        Some("usage") => usage_report::usage_main(&argv[1..]),
        _ => {
            eprintln!(
                "usage: rustsbi-review-cli <checks|resolve-target|run-pass|run-fanout|assemble|archive|usage> ..."
            );
            2
        }
    };
    std::process::exit(code);
}