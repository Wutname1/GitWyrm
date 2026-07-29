//! Regenerates `src/lib/bindings.ts` without launching the app.
//!
//! `tauri dev` exports the bindings on every start, but that requires a full
//! app launch. Run this after changing a `#[specta::specta]` command or a
//! `Type`-deriving struct:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example export_bindings
//! ```

fn main() {
  gitwyrm_lib::export_bindings("../src/lib/bindings.ts").expect("failed to export bindings");
  println!("wrote ../src/lib/bindings.ts");
}
