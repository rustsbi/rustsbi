//! archive subcommand — snapshot a run's artifacts into a timestamped
//! subdirectory. Rust port of scripts/archive_run.py.
//!
//! The run's files stay in the run directory as the overwritable "latest"
//! copy; the timestamped subdirectory preserves the history of each run.
//! Nothing is deleted: older stamps accumulate.

use crate::util::die;
use regex::Regex;

fn stamp_re() -> Regex {
    Regex::new(r"^\d{8}-\d{6}(-\d+)?$").unwrap()
}

fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let mut entries: Vec<_> = std::fs::read_dir(src)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let from = e.path();
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub fn archive_main(argv: &[String]) -> i32 {
    if argv.len() != 1 {
        eprintln!("usage: archive <output-file>");
        return 2;
    }
    let out = std::path::Path::new(&argv[0]);
    let out = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(out)
    };
    let run_dir = out.parent().map(std::path::Path::to_path_buf).unwrap_or_default();

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut dest = run_dir.join(&stamp);
    let mut suffix = 2;
    while dest.exists() {
        // Two runs inside one second.
        dest = run_dir.join(format!("{}-{}", stamp, suffix));
        suffix += 1;
    }
    std::fs::create_dir(&dest).unwrap_or_else(|e| die(&format!("error: mkdir {}: {}", dest.display(), e)));

    let mut count = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap_or_else(|e| die(&format!("error: read {}: {}", run_dir.display(), e)))
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        if path.is_file() {
            std::fs::copy(&path, dest.join(e.file_name()))
                .unwrap_or_else(|e2| die(&format!("error: copy {}: {}", path.display(), e2)));
            count += 1;
        } else if path.is_dir() && !stamp_re().is_match(&e.file_name().to_string_lossy()) {
            // Auxiliary subdirectories of this run belong to the archive;
            // timestamped siblings do not.
            copy_tree(&path, &dest.join(e.file_name()))
                .unwrap_or_else(|e2| die(&format!("error: copy tree {}: {}", path.display(), e2)));
            count += 1;
        }
    }
    println!("{}: {} entries archived", dest.display(), count);
    0
}