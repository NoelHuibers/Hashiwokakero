//! Generates the `i18n` module from the translations in `locales/<lang>/<namespace>.json`.
//! English is the default and the reference: every key has to exist there, other languages
//! fall back to it for keys they don't have yet (with a build warning).

use std::error::Error;
use std::path::PathBuf;

use leptos_i18n_build::{Config, ParseOptions, TranslationsInfos};

/// BCP 47 tags, also the URL prefix of each language (`/de/rules`). English has none.
pub const LOCALES: [&str; 20] = [
    "en", "de", "fr", "it", "es", "pt", "nl", "pl", "tr", "ru", "uk", "ar", "hi", "id", "vi",
    "th", "ja", "ko", "zh", "zh-Hant",
];

pub const NAMESPACES: [&str; 9] =
    ["common", "daily", "practice", "ranks", "account", "create", "lab", "rules", "dojo"];

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=Cargo.toml");

    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR not set")?).join("i18n");
    let cfg = Config::new(LOCALES[0])?
        .add_locales(&LOCALES[1..])?
        .add_namespaces(&NAMESPACES)?
        // Interpolation in plain strings (t_string!, td_string!), for attributes and toasts
        .parse_options(ParseOptions::new().interpolate_display(true));
    let infos = TranslationsInfos::parse(cfg)?;
    infos.emit_diagnostics();
    infos.rerun_if_locales_changed();
    infos.generate_i18n_module(out)?;
    Ok(())
}
