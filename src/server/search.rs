//! Files for search engines at the site's root.
//!
//! The proof that the site is ours, for the consoles of Google (Search Console), Bing (Webmaster
//! Tools) and Yandex (Webmaster), and the key of `IndexNow`, with which the site tells Bing,
//! Yandex and the other `IndexNow` engines about pages that changed (`scripts/indexnow.mjs`,
//! docs/seo.md).
//!
//! Each console offers a file to put on the site, whose name or content is a token; the site
//! serves it at the root while the setting names the token (fly.toml, docs/deployment.md). The
//! tokens are public: anyone can fetch the files, and they only prove ownership to the console
//! that issued them. Files rather than `<meta>` tags, since they don't depend on the pages:
//! the consoles check again from time to time, and a page that changes its head would lose the
//! verification. Unset, there is no file (a 404).

use std::fmt::Write as _;

/// The search engine settings of the server, from the environment. Every one is optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchSettings {
    /// `HASHI_GOOGLE_SITE_VERIFICATION`: the names of the HTML files Search Console asks for,
    /// `google0123456789abcdef.html`.
    pub google: Vec<String>,
    /// `HASHI_BING_SITE_AUTH`: the users of Bing's `BingSiteAuth.xml`, the codes in its `<user>`.
    pub bing: Vec<String>,
    /// `HASHI_YANDEX_VERIFICATION`: the names of the HTML files Yandex Webmaster asks for,
    /// `yandex_0123456789abcdef.html`.
    pub yandex: Vec<String>,
    /// `HASHI_INDEXNOW_KEY`: the site's `IndexNow` key, served as `/<key>.txt`.
    pub indexnow_key: Option<String>,
}

/// A file of the site's root for a search engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFile {
    /// Its path, `/BingSiteAuth.xml`.
    pub path: String,
    pub content_type: &'static str,
    pub body: String,
}

impl SearchSettings {
    /// Reads and checks the settings; a malformed one is an error, never ignored (the server
    /// refuses to start, see `validate_configuration`). Each is a comma separated list, for more
    /// than one owner, but the `IndexNow` key.
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let google = list(
            &var,
            "HASHI_GOOGLE_SITE_VERIFICATION",
            "the name of the file Search Console offers, like google0123456789abcdef.html",
            |name| html_file(name, "google").is_some(),
        )?;
        let bing = list(
            &var,
            "HASHI_BING_SITE_AUTH",
            "the code in the <user> of the BingSiteAuth.xml Bing offers, letters and digits",
            |code| token(code, 1, 64, false),
        )?;
        let yandex = list(
            &var,
            "HASHI_YANDEX_VERIFICATION",
            "the name of the file Yandex offers, like yandex_0123456789abcdef.html",
            |name| html_file(name, "yandex_").is_some(),
        )?;
        let indexnow_key = match setting(&var, "HASHI_INDEXNOW_KEY") {
            None => None,
            Some(key) if token(&key, 8, 128, true) => Some(key),
            Some(key) => {
                return Err(format!(
                    "HASHI_INDEXNOW_KEY: {key:?} is no IndexNow key, 8 to 128 letters, digits and \
                     dashes"
                ));
            }
        };
        Ok(Self {
            google,
            bing,
            yandex,
            indexnow_key,
        })
    }

    /// The files the settings ask for, each with the exact content its console expects.
    pub fn files(&self) -> Vec<SearchFile> {
        let html = "text/html; charset=utf-8";
        let mut files: Vec<SearchFile> = self
            .google
            .iter()
            .map(|name| SearchFile {
                path: format!("/{name}"),
                content_type: html,
                body: format!("google-site-verification: {name}"),
            })
            .collect();
        if !self.bing.is_empty() {
            let users = self.bing.iter().fold(String::new(), |mut users, code| {
                let _ = writeln!(users, "\t<user>{code}</user>");
                users
            });
            files.push(SearchFile {
                path: "/BingSiteAuth.xml".into(),
                content_type: "application/xml",
                body: format!("<?xml version=\"1.0\"?>\n<users>\n{users}</users>\n"),
            });
        }
        files.extend(self.yandex.iter().map(|name| {
            let code = html_file(name, "yandex_").unwrap_or_default();
            SearchFile {
                path: format!("/{name}"),
                content_type: html,
                body: format!(
                    "<html>\n    <head>\n        <meta http-equiv=\"Content-Type\" \
                     content=\"text/html; charset=UTF-8\">\n    </head>\n    <body>Verification: \
                     {code}</body>\n</html>\n"
                ),
            }
        }));
        if let Some(key) = &self.indexnow_key {
            files.push(SearchFile {
                path: format!("/{key}.txt"),
                content_type: "text/plain; charset=utf-8",
                body: key.clone(),
            });
        }
        files
    }
}

/// A setting, trimmed, `None` when unset or empty.
fn setting(var: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    var(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A comma separated setting whose every entry passes `ok`.
fn list(
    var: &impl Fn(&str) -> Option<String>,
    name: &str,
    expected: &str,
    ok: impl Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    let Some(value) = setting(var, name) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(|entry| {
            let entry = entry.trim();
            if ok(entry) {
                Ok(entry.to_string())
            } else {
                Err(format!("{name}: {entry:?} is not {expected}"))
            }
        })
        .collect()
}

/// `min` to `max` ASCII letters and digits, and dashes if `dashes`.
fn token(text: &str, min: usize, max: usize, dashes: bool) -> bool {
    (min..=max).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || (dashes && b == b'-'))
}

/// The code in the name of a console's HTML file, `<prefix><code>.html`: letters, digits, `_`
/// and `-`, so the name is a path of the site's root and nothing else.
fn html_file<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let code = name.strip_prefix(prefix)?.strip_suffix(".html")?;
    (!code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(vars: &[(&str, &str)]) -> Result<SearchSettings, String> {
        SearchSettings::from_env(|name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn without_settings_there_are_no_files() {
        assert_eq!(settings(&[]).unwrap(), SearchSettings::default());
        assert_eq!(SearchSettings::default().files(), []);
        // Empty, as fly.toml has them until the tokens are there
        let empty = settings(&[
            ("HASHI_GOOGLE_SITE_VERIFICATION", ""),
            ("HASHI_BING_SITE_AUTH", " "),
            ("HASHI_YANDEX_VERIFICATION", ""),
            ("HASHI_INDEXNOW_KEY", ""),
        ])
        .unwrap();
        assert_eq!(empty.files(), []);
    }

    /// Exactly what each console asks to upload.
    #[test]
    fn each_console_gets_its_file() {
        let configured = settings(&[
            (
                "HASHI_GOOGLE_SITE_VERIFICATION",
                "google0123456789abcdef.html, googleZ-9_x.html",
            ),
            ("HASHI_BING_SITE_AUTH", "0123456789ABCDEF0123456789ABCDEF"),
            ("HASHI_YANDEX_VERIFICATION", "yandex_0123456789abcdef.html"),
            (
                "HASHI_INDEXNOW_KEY",
                " 3f2b8a9c-4d1e-4f6a-9b7c-0e5d1a2b3c4d ",
            ),
        ])
        .unwrap();
        let files = configured.files();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/google0123456789abcdef.html",
                "/googleZ-9_x.html",
                "/BingSiteAuth.xml",
                "/yandex_0123456789abcdef.html",
                "/3f2b8a9c-4d1e-4f6a-9b7c-0e5d1a2b3c4d.txt",
            ]
        );
        assert_eq!(
            files[0].body,
            "google-site-verification: google0123456789abcdef.html"
        );
        assert_eq!(files[0].content_type, "text/html; charset=utf-8");
        assert_eq!(
            files[2].body,
            "<?xml version=\"1.0\"?>\n<users>\n\t<user>0123456789ABCDEF0123456789ABCDEF</user>\n</users>\n"
        );
        assert_eq!(files[2].content_type, "application/xml");
        assert!(
            files[3]
                .body
                .contains("<body>Verification: 0123456789abcdef</body>"),
            "{}",
            files[3].body
        );
        assert_eq!(files[4].body, "3f2b8a9c-4d1e-4f6a-9b7c-0e5d1a2b3c4d");
        assert_eq!(files[4].content_type, "text/plain; charset=utf-8");

        // More than one Bing user: one file, a <user> each
        let two = settings(&[("HASHI_BING_SITE_AUTH", "AAAA,BBBB")]).unwrap();
        assert_eq!(
            two.files()[0].body,
            "<?xml version=\"1.0\"?>\n<users>\n\t<user>AAAA</user>\n\t<user>BBBB</user>\n</users>\n"
        );
    }

    /// Nothing that would make another path than a file at the root, or a broken file.
    #[test]
    fn malformed_tokens_are_errors() {
        for (name, value) in [
            ("HASHI_GOOGLE_SITE_VERIFICATION", "0123456789abcdef"),
            ("HASHI_GOOGLE_SITE_VERIFICATION", "google.html"),
            ("HASHI_GOOGLE_SITE_VERIFICATION", "google0123.htm"),
            ("HASHI_GOOGLE_SITE_VERIFICATION", "google../admin.html"),
            ("HASHI_GOOGLE_SITE_VERIFICATION", "google01/23.html"),
            (
                "HASHI_GOOGLE_SITE_VERIFICATION",
                "google0123456789abcdef.html,",
            ),
            ("HASHI_BING_SITE_AUTH", "<user>ABC</user>"),
            ("HASHI_BING_SITE_AUTH", "ABC-DEF"),
            ("HASHI_YANDEX_VERIFICATION", "yandex0123.html"),
            ("HASHI_YANDEX_VERIFICATION", "yandex_0123.html.txt"),
            ("HASHI_INDEXNOW_KEY", "short"),
            ("HASHI_INDEXNOW_KEY", "has a space in it"),
            ("HASHI_INDEXNOW_KEY", "a,b,c,d,e,f,g,h"),
            ("HASHI_INDEXNOW_KEY", &"k".repeat(129)),
        ] {
            let error = settings(&[(name, value)]).unwrap_err();
            assert!(error.starts_with(name), "{value}: {error}");
        }
        assert!(settings(&[("HASHI_INDEXNOW_KEY", &"k".repeat(128))]).is_ok());
        assert!(settings(&[("HASHI_INDEXNOW_KEY", "abcd1234")]).is_ok());
    }

    /// The settings fly.toml deploys: a malformed token there would stop the server after the
    /// deploy, this fails first.
    #[test]
    fn the_deployed_settings_are_valid() {
        let fly = include_str!("../../fly.toml");
        let env: Vec<(&str, &str)> = fly
            .lines()
            .filter_map(|line| {
                let (name, value) = line.trim().split_once(" = ")?;
                Some((name, value.strip_prefix('"')?.strip_suffix('"')?))
            })
            .collect();
        for name in [
            "HASHI_GOOGLE_SITE_VERIFICATION",
            "HASHI_BING_SITE_AUTH",
            "HASHI_YANDEX_VERIFICATION",
            "HASHI_INDEXNOW_KEY",
        ] {
            assert!(env.iter().any(|(n, _)| *n == name), "{name} in fly.toml");
        }
        if let Err(e) = settings(&env) {
            panic!("fly.toml: {e}");
        }
    }
}
