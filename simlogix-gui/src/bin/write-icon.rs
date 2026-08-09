//! Writes the application icon out as a PNG.
//!
//! Kept as a tool rather than a build step: the icon changes about once, and
//! a generated file that regenerates only when asked is easier to reason
//! about than one a build script rewrites under you.
//!
//! `cargo run -p simlogix-gui --bin write-icon -- assets/icon.png`
//!
//! The module is included by path rather than through a library target,
//! which would mean splitting the crate in two for the sake of one tool.

// `paint` is for the About box and has no business here; the tool only
// wants the encoder.
#[allow(dead_code)]
#[path = "../icon.rs"]
mod icon;

fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/icon.png".to_string());
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, icon::png::encode())?;
    println!("wrote {path}");
    Ok(())
}
