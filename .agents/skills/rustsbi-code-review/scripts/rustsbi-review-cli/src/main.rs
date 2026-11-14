//! rustsbi-review-cli — the whole Rust script layer of the rustsbi-code-review
//! skill: deterministic checks plus LLM review orchestration.
//!
//! Subcommands:
//!   checks          deterministic check pipeline: scope -> convention -> build -> lint -> test -> report
//!   resolve-target  build the canonical review input from a diff or file set
//!   run-pass        spawn one review pass and collect its comments and token usage
//!   run-fanout      combined or per-persona fan-out, merged and de-duplicated
//!   assemble        merge a pass's comments into the review file
//!   archive         snapshot a run into a timestamped subdirectory
//!   usage           token accounting report over .usage.json sidecars

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
mod score;
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
        Some("score") => score::score_main(&argv[1..]),
        _ => {
            eprintln!(
                "usage: rustsbi-review-cli <checks|resolve-target|run-pass|run-fanout|assemble|archive|usage> ..."
            );
            2
        }
    };
    std::process::exit(code);
}
