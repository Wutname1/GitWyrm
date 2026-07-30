//! Regenerates `src/lib/bindings.ts` without launching the app.
//!
//! Run after changing a `#[specta::specta]` command or a `Type`-deriving struct:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings
//! ```
//!
//! This is a `[[bin]]` rather than an example on purpose: the lib is built as a
//! `cdylib` for Tauri, and an example links against that DLL, which exports no
//! Rust symbols and dies with STATUS_ENTRYPOINT_NOT_FOUND on Windows. A bin
//! target links the rlib instead.

fn main() {
  gitwyrm_lib::export_bindings("../src/lib/bindings.ts").expect("failed to export bindings");
  println!("wrote src/lib/bindings.ts");
}
