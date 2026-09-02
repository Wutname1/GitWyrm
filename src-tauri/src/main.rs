// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Git spawns this same binary to ask for a credential. That mode must be
    // handled before anything else: building a window, registering plugins, or
    // tripping the single-instance guard would all be wrong for a process whose
    // entire job is to print one line and exit. See git::credential_helper.
    let args: Vec<String> = std::env::args().collect();
    if gitwyrm_lib::is_credential_helper(&args) {
        std::process::exit(gitwyrm_lib::run_credential_helper(&args));
    }

    gitwyrm_lib::run();
}
