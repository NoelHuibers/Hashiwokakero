//! Impressum and Datenschutzerklärung. German law applies, so the German text is the binding one;
//! an English version follows. Parts in [brackets] still have to be filled in.

use leptos::prelude::*;

use crate::components::ui::separator::Separator;
use crate::i18n::{t, t_string, use_i18n};
use crate::seo::PageMeta;

type Section = (&'static str, &'static [&'static str]);

const IMPRESSUM_DE: &[Section] = &[
    (
        "Angaben gemäß § 5 DDG",
        &[
            "Noel Huibers",
            "[Straße und Hausnummer]",
            "[PLZ und Ort]",
            "Deutschland",
        ],
    ),
    ("Kontakt", &["E-Mail: [E-Mail-Adresse]"]),
    (
        "Verantwortlich für den Inhalt nach § 18 Abs. 2 MStV",
        &["Noel Huibers, [Anschrift wie oben]"],
    ),
    (
        "Verbraucherstreitbeilegung",
        &[
            "Wir sind nicht bereit oder verpflichtet, an Streitbeilegungsverfahren vor einer Verbraucherschlichtungsstelle teilzunehmen.",
        ],
    ),
];

const IMPRESSUM_EN: &[Section] = &[
    (
        "Information according to § 5 DDG (German Digital Services Act)",
        &[
            "Noel Huibers",
            "[Street and number]",
            "[Postcode and city]",
            "Germany",
        ],
    ),
    ("Contact", &["Email: [email address]"]),
    (
        "Responsible for the content according to § 18 (2) MStV",
        &["Noel Huibers, [address as above]"],
    ),
];

const PRIVACY_DE: &[Section] = &[
    (
        "Verantwortlicher",
        &["Noel Huibers, [Anschrift], E-Mail: [E-Mail-Adresse]"],
    ),
    (
        "Kurz gesagt",
        &[
            "Hashi kommt ohne Werbung, Tracking und Analyse-Tools aus. Wir verarbeiten nur die Daten, die für das Spiel nötig sind.",
        ],
    ),
    (
        "Hosting",
        &[
            "Die Website läuft bei Fly.io (Fly.io, Inc., USA, Rechenzentrum in Amsterdam). Beim Aufruf werden technisch notwendige Daten wie IP-Adresse, Zeitpunkt und aufgerufene Seite verarbeitet und kurzzeitig in Server-Logs gespeichert (Art. 6 Abs. 1 lit. f DSGVO, Betrieb und Sicherheit der Website). [Auftragsverarbeitungsvertrag und Grundlage der Übermittlung in die USA ergänzen.]",
        ],
    ),
    (
        "Konto, Tagesrätsel und Rangliste",
        &[
            "Wenn du das Tagesrätsel spielst, legen wir ein Gastkonto an und setzen das Cookie „hashi_session“, damit dein Ergebnis dir zugeordnet bleibt. Gespeichert werden ein zufälliger Gastname, deine Ergebnisse (Zeit, Züge, Tipps) und ein Hash des Sitzungs-Tokens.",
            "Bei einer Registrierung speichern wir zusätzlich deinen Benutzernamen und einen Argon2-Hash deines Passworts. Benutzername bzw. Gastname und Zeiten erscheinen öffentlich in der Rangliste.",
            "Die Daten liegen in einer Datenbank bei Turso (ChiselStrike, Inc.). Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Nutzung des Spiels). [Auftragsverarbeitung und Speicherort ergänzen.]",
        ],
    ),
    (
        "Cookies und lokaler Speicher",
        &[
            "Neben dem Sitzungs-Cookie merkt sich das Cookie „i18n_pref_locale“ deine Sprache. Im lokalen Speicher deines Browsers liegen das Farbschema, die Toneinstellungen und dein aktuelles Spiel. Alle sind technisch notwendig (§ 25 Abs. 2 TDDDG).",
        ],
    ),
    (
        "Faires Spiel",
        &[
            "Damit die Rangliste fair bleibt, speichern wir zu jedem Tagesrätsel das Protokoll deiner Züge (welche Brücke wann gesetzt wurde). Dein Browser legt dafür eine zufällige Geräte-ID im lokalen Speicher ab; der Server speichert sie nur als Hash, der jeden Tag anders ist, und erkennt damit nur, ob dasselbe Gerät das Rätsel schon mit einem anderen Konto gestartet hat.",
            "Bei Einsendungen im SAT Dojo speichern wir einen Hash deiner IP-Adresse, ebenfalls täglich anders und nicht umkehrbar, um die Zahl der Einsendungen pro Netz zu begrenzen. Rechtsgrundlage ist Art. 6 Abs. 1 lit. f DSGVO (Schutz vor Betrug).",
        ],
    ),
    (
        "Stempel",
        &[
            "Für die Stempel im Stempelheft werten wir deine Ergebnisse und das Protokoll deiner Züge aus, etwa deine Zeit, deinen Platz oder ob du eine Brücke wieder weggenommen hast. Deine Stempel sind auf deinem Spielerprofil und in den Ranglisten öffentlich sichtbar.",
            "Für die Stempel zur Tageszeit schickt dein Browser beim Lösen eines Tagesrätsels seine Zeitzone mit. Wir speichern die Zeitzone nicht, sondern nur, ob du nachts (0–4 Uhr) oder mittags (12–14 Uhr) deiner Ortszeit fertig geworden bist. Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Nutzung des Spiels).",
        ],
    ),
    (
        "Schriftarten",
        &[
            "Alle Schriftarten liegen auf unserem eigenen Server. Es werden keine Daten an Google Fonts oder andere Drittanbieter übermittelt.",
        ],
    ),
    (
        "Deine Rechte",
        &[
            "Du hast das Recht auf Auskunft, Berichtigung, Löschung, Einschränkung der Verarbeitung, Datenübertragbarkeit und Widerspruch (Art. 15–21 DSGVO) sowie auf Beschwerde bei einer Datenschutz-Aufsichtsbehörde. Schreib uns dafür einfach eine E-Mail.",
        ],
    ),
];

const PRIVACY_EN: &[Section] = &[
    (
        "Controller",
        &["Noel Huibers, [address], email: [email address]"],
    ),
    (
        "In short",
        &["Hashi has no ads, tracking or analytics. We only process the data the game needs."],
    ),
    (
        "Hosting",
        &[
            "The site is hosted by Fly.io (Fly.io, Inc., USA, data centre in Amsterdam). Requests are processed with technically necessary data such as IP address, time and page, kept briefly in server logs (Art. 6 (1) f GDPR).",
        ],
    ),
    (
        "Account, daily puzzle and leaderboard",
        &[
            "Playing the daily creates a guest account with the cookie \"hashi_session\". We store a random guest name, your results and a hash of the session token; registered accounts add a username and an Argon2 password hash. Names and times are public on the leaderboard. The database is hosted by Turso (Art. 6 (1) b GDPR).",
        ],
    ),
    (
        "Cookies and local storage",
        &[
            "The cookie \"i18n_pref_locale\" remembers your language; your colour scheme, sound settings and current game stay in your browser's local storage. All of these are strictly necessary.",
        ],
    ),
    (
        "Fair play",
        &[
            "To keep the leaderboard fair we store the log of your moves for each daily (which bridge was set when). Your browser keeps a random device ID in local storage for this; the server stores it only as a hash that changes every day, and uses it only to see whether the same device already started the daily with another account. For SAT Dojo submissions we store a hash of your IP address, also different every day and not reversible, to limit submissions per network (Art. 6 (1) f GDPR, preventing fraud).",
        ],
    ),
    (
        "Stamps",
        &[
            "For the stamps in the stamp book we look at your results and your move log, such as your time, your place or whether you took a bridge away again. Your stamps are public on your player profile and on the leaderboards. For the time-of-day stamps your browser sends its time zone when you finish a daily; we don't store the time zone, only whether you finished at night (midnight to 4 am) or over lunch (noon to 2 pm) in your local time (Art. 6 (1) b GDPR).",
        ],
    ),
    (
        "Fonts",
        &[
            "All fonts are served from our own server, nothing is sent to Google Fonts or any other third party.",
        ],
    ),
    (
        "Your rights",
        &[
            "You have the right to access, rectification, erasure, restriction, portability and objection (Art. 15–21 GDPR) and to lodge a complaint with a supervisory authority. Just send us an email.",
        ],
    ),
];

#[component]
fn Sections(sections: &'static [Section]) -> impl IntoView {
    sections
        .iter()
        .map(|(heading, paragraphs)| {
            view! {
                <section class="flex flex-col gap-1.5">
                    <h2 class="text-lg font-bold">{*heading}</h2>
                    {paragraphs
                        .iter()
                        .map(|p| view! { <p class="leading-relaxed text-muted-foreground">{*p}</p> })
                        .collect_view()}
                </section>
            }
        })
        .collect_view()
}

#[component]
fn LegalPage(
    title: Signal<String>,
    kanji: &'static str,
    path: &'static str,
    de: &'static [Section],
    en: &'static [Section],
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <PageMeta title=Signal::derive(move || format!("{} · Hashi", title.get())) description=title path />
        <section class="mx-auto flex max-w-3xl flex-col gap-6 px-4 py-8 sm:px-6">
            <div>
                <p class="font-pixel text-sm text-primary">{kanji}</p>
                <h1 class="text-3xl font-black">{move || title.get()}</h1>
            </div>
            <div class="flex flex-col gap-5" lang="de">
                <Sections sections=de />
            </div>
            <Separator />
            <div class="flex flex-col gap-5" lang="en">
                <p class="text-sm font-bold uppercase tracking-wider text-muted-foreground">
                    {t!(i18n, common.legal.english)}
                </p>
                <Sections sections=en />
            </div>
        </section>
    }
}

#[component]
pub fn ImpressumPage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <LegalPage
            title=Signal::derive(move || t_string!(i18n, common.legal.imprint).to_string())
            kanji="奥付 · okuzuke"
            path="/impressum"
            de=IMPRESSUM_DE
            en=IMPRESSUM_EN
        />
    }
}

#[component]
pub fn PrivacyPage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <LegalPage
            title=Signal::derive(move || t_string!(i18n, common.legal.privacy).to_string())
            kanji="個人情報 · kojin jōhō"
            path="/datenschutz"
            de=PRIVACY_DE
            en=PRIVACY_EN
        />
    }
}
