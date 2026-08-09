//! Man page generation binary for bead-rs
//!
//! This binary generates man pages from the clap command tree.
//! Run with: cargo run --bin generate-man-pages

use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get output directory (default: man/man1)
    let args: Vec<String> = env::args().collect();
    let out_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        let mut dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        dir.push("man");
        dir.push("man1");
        dir
    };

    println!("Generating man pages to: {}", out_dir.display());

    bead_rs::docs::generate_man_pages(&out_dir)?;

    println!("Man pages generated successfully!");
    Ok(())
}
