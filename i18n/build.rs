//! Generates the `i18n` module of this crate from the translations in
//! `locales/<lang>/<namespace>.json` at the workspace root.
//! English is the default and the reference: every key has to exist there, other languages
//! fall back to it for keys they don't have yet (with a build warning).
//!
//! `HASHI_LOCALES=en,ja` translates only the listed languages, for lighter local builds: the
//! others get empty translations and show English. They stay in the `Locale` enum, so the code
//! and the URLs (`/de/rules`) don't change, but `leptos_i18n` generates no code of their own for
//! them. Unset (CI, releases, Docker), every language is built.

use std::error::Error;
use std::path::{Path, PathBuf};

use leptos_i18n_build::{Config, ParseOptions, TranslationsInfos};

/// BCP 47 tags, also the URL prefix of each language (`/de/rules`). English has none.
pub const LOCALES: [&str; 20] = [
    "en", "de", "fr", "it", "es", "pt", "nl", "pl", "tr", "ru", "uk", "ar", "hi", "id", "vi", "th",
    "ja", "ko", "zh", "zh-Hant",
];

pub const NAMESPACES: [&str; 16] = [
    "common", "daily", "practice", "ranks", "account", "create", "lab", "rules", "dojo", "stamps",
    "contact", "groups", "replay", "rivals", "banzuke", "hanko",
];

/// At the workspace root, relative to this crate
const LOCALES_DIR: &str = "../locales";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-env-changed=HASHI_LOCALES");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR not set")?);
    let mut options = ParseOptions::new()
        // Interpolation in plain strings (t_string!, td_string!), for attributes and toasts
        .interpolate_display(true);
    let mut cfg = Config::new(LOCALES[0])?
        .locales_path(LOCALES_DIR)
        .add_locales(&LOCALES[1..])?
        .add_namespaces(NAMESPACES)?;
    let subset = std::env::var("HASHI_LOCALES")
        .ok()
        .filter(|s| !s.trim().is_empty());
    if let Some(subset) = &subset {
        let wanted = wanted_locales(subset)?;
        println!(
            "cargo::warning=HASHI_LOCALES={}: the other languages show English. Unset it for \
             tests and release builds.",
            wanted.join(",")
        );
        cfg = cfg.locales_path(stub_locales(&out_dir, &wanted)?);
        // Every key of the stubbed languages is missing, on purpose. This also hides the missing
        // keys of the wanted ones, the test translations_match_english still finds them.
        options = options.suppress_key_warnings(true);
    }
    let infos = TranslationsInfos::parse(cfg.parse_options(options))?;
    infos.emit_diagnostics();
    if subset.is_none() {
        infos.rerun_if_locales_changed();
    }
    infos.generate_i18n_module(out_dir.join("i18n"))?;
    Ok(())
}

/// The languages in `HASHI_LOCALES`, English always among them (it's the reference)
fn wanted_locales(list: &str) -> Result<Vec<&'static str>, Box<dyn Error>> {
    let mut wanted = vec![LOCALES[0]];
    for tag in list.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let locale = LOCALES
            .iter()
            .find(|l| l.eq_ignore_ascii_case(tag))
            .ok_or_else(|| {
                format!("HASHI_LOCALES: unknown language {tag:?}, known: {LOCALES:?}")
            })?;
        if !wanted.contains(locale) {
            wanted.push(locale);
        }
    }
    Ok(wanted)
}

/// A copy of `locales/` in `OUT_DIR` with the wanted languages, and `{}` for every other one.
/// The build reruns when the originals of the wanted ones change, not the copies: cargo would
/// see them written during the run and rerun every time.
fn stub_locales(out_dir: &Path, wanted: &[&str]) -> Result<PathBuf, Box<dyn Error>> {
    let manifest_dir =
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR not set")?;
    let source = Path::new(&manifest_dir).join(LOCALES_DIR);
    let target = out_dir.join("locales");
    for locale in LOCALES {
        let dir = target.join(locale);
        std::fs::create_dir_all(&dir)?;
        for namespace in NAMESPACES {
            let file = format!("{namespace}.json");
            if wanted.contains(&locale) {
                let original = source.join(locale).join(&file);
                println!("cargo::rerun-if-changed={}", original.display());
                std::fs::copy(&original, dir.join(&file))?;
            } else {
                std::fs::write(dir.join(&file), "{}")?;
            }
        }
    }
    Ok(target)
}
