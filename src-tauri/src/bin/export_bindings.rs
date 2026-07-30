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
  // Resolve against the manifest directory, not the working directory. With
  // `--manifest-path` from the repo root, cargo runs this with the cwd at the
  // root, so a relative `../src/lib/bindings.ts` lands one level ABOVE the repo
  // and the real file is never updated -- while the command still prints
  // success. `tauri dev` happens to run with the cwd at src-tauri, which is why
  // the bug only appears when regenerating by hand.
  let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("src-tauri has a parent")
    .join("src")
    .join("lib")
    .join("bindings.ts");
  let out = out.to_string_lossy().into_owned();
  gitwyrm_lib::export_bindings(&out).expect("failed to export bindings");
  println!("wrote {out}");
}
