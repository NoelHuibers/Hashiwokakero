//! Golden files: the exact text of something the site keeps or sends, checked in under
//! `src/golden/files/`. JSON in the database, in the browser's local storage and between the
//! server, the pages and the Dojo worker outlives a deploy: rows stay, open tabs keep their old
//! code, the worker deploys separately. An update of serde, `serde_json` or a change of one of our
//! types has to keep producing (and reading) the same text.
//!
//! For an intended change, `GOLDEN_UPDATE=1 cargo test ...` writes the current text instead.
//! Before committing it, make sure the old text still loads (keep a test with it).

mod wire;

pub(crate) use wire::puzzle;

use std::fmt::Debug;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/golden/files")
        .join(name)
}

/// Compares `actual` with the golden file `name`. `meaning` says what a difference would do in
/// production, it is the first line of the failure.
pub(crate) fn check(name: &str, actual: &str, meaning: &str) {
    let file = path(name);
    if std::env::var_os("GOLDEN_UPDATE").is_some() {
        std::fs::write(&file, format!("{actual}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "{name}: no golden file {}, run the test with GOLDEN_UPDATE=1 to write it",
            file.display()
        )
    });
    assert!(
        expected.trim_end() == actual,
        "{name}: {meaning}\n\nIf the change is intended and the old text still loads, rewrite the \
         file with GOLDEN_UPDATE=1.\n\nexpected ({}):\n{}\n\nactual:\n{actual}\n",
        file.display(),
        expected.trim_end(),
    );
}

/// `value` as pretty JSON has to match the golden file, and the golden file has to load as
/// `value` again.
pub(crate) fn check_json<T>(name: &str, value: &T, meaning: &str)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    check(name, &serde_json::to_string_pretty(value).unwrap(), meaning);
    let stored = std::fs::read_to_string(path(name)).unwrap();
    let loaded: T = serde_json::from_str(&stored).unwrap_or_else(|e| {
        panic!("{name}: the golden file doesn't load any more ({e}). {meaning}")
    });
    assert_eq!(
        &loaded, value,
        "{name}: the golden file loads differently. {meaning}"
    );
}
