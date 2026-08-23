//! Build the standalone TLHdig Beta 0.3 package.
//!
//! ```text
//! cargo run --release --example export_beta -- <archive.zip> [destination]
//! ```
//!
//! Produces `<destination>/TLHdig_Beta_0.3` — an inventory beside one directory
//! per CTH group, each holding the normalised documents of that group. It opens
//! by double-clicking and needs no server: every link is relative, so moving the
//! folder keeps them working.
//!
//! Arguments, a call and a report: everything else is [`aruna::export`], where
//! it can be tested against a four-document archive and measured without one.
//! This file used to hold the pipeline, and nothing in it could be reached by a
//! test.

use aruna::export::{self, PACKAGE};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(zip) = args.next().map(PathBuf::from) else {
        eprintln!("usage: export_beta <archive.zip> [destination]");
        return ExitCode::FAILURE;
    };
    let destination = args
        .next()
        .map(PathBuf::from)
        .or_else(dirs::download_dir)
        .unwrap_or_else(|| PathBuf::from("."));

    eprintln!("Building {PACKAGE} from {}…", zip.display());
    match export::build(
        &zip,
        &destination,
        aruna::SOURCE_LABEL,
        &aruna::job::Job::unattended(),
    ) {
        Ok(built) => {
            println!();
            println!("package:          {}", destination.join(PACKAGE).display());
            println!("CTH groups:       {}", built.groups);
            println!("documents:        {}", built.documents);
            println!("fragment links:   {}", built.fragment_links);
            println!("disambiguated:    {}", built.disambiguated);
            println!("stylesheet PIs dropped: {}", built.stylesheet_dropped);
            println!("validation errors: 0");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("\nBUILD FAILED: {err}");
            ExitCode::FAILURE
        }
    }
}
