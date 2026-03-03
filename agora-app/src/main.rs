#![warn(
    missing_docs,
    rust_2018_idioms,
    unused_import_braces,
    unused_qualifications,
    clippy::all,
    clippy::pedantic
)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agora_app::run()
}
