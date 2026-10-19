//! Impressum and Datenschutzerklärung. German law applies, so the German text is the binding one;
//! an English version follows. Email addresses and paths (`/kontakt`) in the text become links.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::ui::separator::Separator;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::localized;
use crate::seo::PageMeta;

type Section = (&'static str, &'static [&'static str]);

/// Where people reach us by email, also on the contact page.
pub const CONTACT_EMAIL: &str = "online-impressum.d50dw@passmail.net";

const IMPRESSUM_DE: &[Section] = &[
    (
        "Angaben gemäß § 5 DDG",
        &[
            "Noel Huibers",
            "c/o Online-Impressum 12002",
            "Europaring 90",
            "53757 Sankt Augustin",
            "Deutschland",
        ],
    ),
    (
        "Kontakt",
        &[
            "E-Mail: online-impressum.d50dw@passmail.net",
            "Kontaktformular: /kontakt",
        ],
    ),
];

const IMPRESSUM_EN: &[Section] = &[
    (
        "Information according to § 5 DDG (German Digital Services Act)",
        &[
            "Noel Huibers",
            "c/o Online-Impressum 12002",
            "Europaring 90",
            "53757 Sankt Augustin",
            "Germany",
        ],
    ),
    (
        "Contact",
        &[
            "Email: online-impressum.d50dw@passmail.net",
            "Contact form: /kontakt",
        ],
    ),
];

const PRIVACY_DE: &[Section] = &[
    (
        "Verantwortlicher",
        &[
            "Noel Huibers, c/o Online-Impressum 12002, Europaring 90, 53757 Sankt Augustin, Deutschland",
            "E-Mail: online-impressum.d50dw@passmail.net, Kontaktformular: /kontakt",
        ],
    ),
    (
        "Kurz gesagt",
        &[
            "Hashi kommt ohne Werbung, Tracking und Analyse-Tools aus. Wir verarbeiten nur die Daten, die für das Spiel nötig sind, und geben sie an niemanden weiter außer an die Dienstleister, die die Website für uns betreiben (Fly.io, Turso und Proton, siehe unten).",
            "Üben, die Rätselwerkstatt und das Solver-Labor funktionieren ganz ohne Konto. Rätsel, die du dort erzeugen oder lösen lässt, verarbeitet der Server nur für die Antwort und speichert sie nicht.",
        ],
    ),
    (
        "Hosting",
        &[
            "Die Website läuft bei Fly.io, Inc. (USA) auf Servern in Amsterdam. Bei jedem Aufruf verarbeitet Fly.io technisch notwendige Daten wie deine IP-Adresse, den Zeitpunkt und die aufgerufene Seite, um die Seite auszuliefern und vor Angriffen zu schützen. Unsere Anwendung selbst schreibt keine Zugriffsprotokolle. Rechtsgrundlage ist Art. 6 Abs. 1 lit. f DSGVO; unser berechtigtes Interesse ist der sichere Betrieb der Website.",
            "Fly.io verarbeitet die Daten in unserem Auftrag (Art. 28 DSGVO). Fly.io ist nach dem EU-U.S. Data Privacy Framework zertifiziert, Übermittlungen in die USA stützen sich auf den Angemessenheitsbeschluss der EU-Kommission (Art. 45 DSGVO).",
        ],
    ),
    (
        "Datenbank",
        &[
            "Konten, Ergebnisse, Stempel, Dojo-Einsendungen und Kontaktnachrichten liegen in einer Datenbank von Turso (ChiselStrike Inc., 2093 Philadelphia Pike #6336, Claymont, Delaware 19703, USA) in einem Rechenzentrum von Amazon Web Services in Irland. Turso verarbeitet die Daten in unserem Auftrag (Art. 28 DSGVO). Für mögliche Zugriffe aus den USA gelten die EU-Standardvertragsklauseln (Art. 46 Abs. 2 lit. c DSGVO).",
        ],
    ),
    (
        "Gastkonto und Konto",
        &[
            "Wenn du die Brücke des Tages startest, legen wir automatisch ein Gastkonto mit einem zufälligen Gastnamen an und setzen das Cookie „hashi_session“, damit deine Ergebnisse dir zugeordnet bleiben. Es enthält einen zufälligen Schlüssel, den wir nur als Hash speichern, und läuft ein Jahr nach deinem letzten Besuch ab, beim Abmelden sofort.",
            "Bei einer Registrierung speichern wir zusätzlich deinen Benutzernamen und einen Argon2-Hash deines Passworts, nie das Passwort selbst. Meldest du dich an, nachdem du als Gast gespielt hast, übernehmen wir die Ergebnisse des Gastkontos in dein Konto. Fehlgeschlagene Anmeldungen zählen wir 15 Minuten lang im Arbeitsspeicher, um das Erraten von Passwörtern zu bremsen.",
            "Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Nutzung des Spiels), für die Begrenzung der Anmeldeversuche Art. 6 Abs. 1 lit. f DSGVO (Schutz deines Kontos).",
        ],
    ),
    (
        "Ranglisten und Spielerprofil",
        &[
            "Zu jeder Brücke des Tages speichern wir Start- und Endzeit, deine Zeit, deine Züge, wie oft du Tipps und Prüfungen genutzt hast, und die gelöste Brückenbelegung.",
            "Die Brücke des Tages ist ein öffentlicher Wettbewerb. Öffentlich sichtbar sind daher dein Name (Benutzer- oder Gastname) und ob du als Gast spielst, in der Tagesrangliste deine Zeit, Tipps, Prüfungen und dein gewählter Stempel, in der Ruhmeshalle und im Monatsrennen deine gelösten Rätsel, Serien, Medaillen und Punkte, und dein Spielerprofil mit dem Datum, seit dem du spielst, deiner Bilanz, den letzten 35 Tagen mit Zeit und Platz und deinem Stempelheft. Spielerprofile sind für Suchmaschinen gesperrt.",
            "Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Nutzung des Spiels). Wenn du nicht öffentlich erscheinen möchtest, spiel die Übungsrätsel.",
        ],
    ),
    (
        "Faires Spiel",
        &[
            "Damit die Rangliste fair bleibt, speichern wir zu jeder Brücke des Tages das Protokoll deiner Züge (welche Brücke wann gesetzt wurde). Dein Browser legt dafür außerdem eine zufällige Geräte-ID im lokalen Speicher ab und schickt sie beim Start mit. Wir speichern sie nur als Hash, der mit einem geheimen Schlüssel gebildet wird und jeden Tag anders ist, und prüfen damit nur, ob dasselbe Gerät das Rätsel schon mit einem anderen Konto gestartet hat.",
            "Ein Programm prüft jedes Ergebnis automatisch, etwa auf unmöglich schnelle Lösungen, ein unnatürliches Klicktempo oder ein zweites Konto auf demselben Gerät. Auffällige Ergebnisse erscheinen nicht in der Rangliste. Außerdem kann der Betreiber Ergebnisse von Hand prüfen und aus der Rangliste nehmen. Wenn du meinst, dass dein Ergebnis zu Unrecht nicht gewertet wurde, schreib uns, wir sehen es uns an.",
            "Rechtsgrundlage ist Art. 6 Abs. 1 lit. f DSGVO; unser berechtigtes Interesse ist eine faire Rangliste ohne Betrug.",
        ],
    ),
    (
        "Stempel",
        &[
            "Für die Stempel im Stempelheft werten wir deine Ergebnisse und das Protokoll deiner Züge aus, etwa deine Zeit, deinen Platz oder ob du eine Brücke wieder weggenommen hast, außerdem dein Monatsrennen, deine Teilnahme am SAT Dojo und das Alter deines Kontos. Deine Stempel sind mit dem Datum, an dem du sie bekommen hast, auf deinem Spielerprofil öffentlich.",
            "Für die Stempel zur Tageszeit schickt dein Browser beim Lösen den Zeitunterschied deiner Ortszeit zu UTC mit. Wir speichern ihn nicht, sondern nur, ob du nachts (0–4 Uhr) oder mittags (12–14 Uhr) deiner Ortszeit fertig geworden bist. Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Nutzung des Spiels).",
        ],
    ),
    (
        "SAT Dojo",
        &[
            "Mit einem registrierten Konto kannst du im SAT Dojo einen eigenen Löser als WebAssembly-Datei einsenden. Wir speichern die Datei, Name, Beschreibung und Quellcode-Link des Lösers, den Zeitpunkt und das Ergebnis. Öffentlich sichtbar sind Name, Beschreibung, Link, dein Benutzername, Datum, Punktzahl, Prüfsumme und Größe der Datei; die Datei selbst geben wir nicht heraus. Bitte beachte, dass WebAssembly-Dateien Debug-Informationen wie lokale Dateipfade enthalten können; wir speichern die Datei so, wie du sie hochlädst.",
            "Zum Bewerten schicken wir nur die Datei, ohne Angaben zu dir, an einen eigenen Rechenserver, ebenfalls bei Fly.io in Amsterdam. Neue Einsendungen vergleichen wir automatisch mit früheren, um Kopien zu erkennen. Rechtsgrundlage ist Art. 6 Abs. 1 lit. b DSGVO (Teilnahme am Dojo).",
            "Um die Zahl der Einsendungen pro Netz zu begrenzen, speichern wir einen Hash deiner IP-Adresse, der mit einem geheimen Schlüssel gebildet wird und jeden Tag anders ist; die IP-Adresse selbst speichern wir nicht. Rechtsgrundlage ist Art. 6 Abs. 1 lit. f DSGVO (Schutz vor Missbrauch).",
        ],
    ),
    (
        "Kontaktformular und E-Mail",
        &[
            "Wenn du uns über das Kontaktformular schreibst, speichern wir deine Nachricht, deinen Namen und deine E-Mail-Adresse, falls du sie angibst, sowie einen Hash deiner IP-Adresse, der mit einem geheimen Schlüssel gebildet wird und jeden Tag anders ist, um die Zahl der Nachrichten pro Netz zu begrenzen.",
            "Die Nachricht wird uns außerdem per E-Mail über Proton Mail zugestellt (Proton AG, Route de la Galaise 32, 1228 Plan-les-Ouates, Schweiz). Proton verarbeitet die E-Mails in unserem Auftrag (Art. 28 DSGVO); für die Schweiz besteht ein Angemessenheitsbeschluss der EU-Kommission (Art. 45 DSGVO). Dasselbe gilt, wenn du uns direkt eine E-Mail schreibst.",
            "Wir nutzen deine Angaben nur, um deine Anfrage zu beantworten. Nachrichten in der Datenbank löschen wir, sobald sie erledigt sind, spätestens nach sechs Monaten; E-Mails, sobald wir sie nicht mehr brauchen. Rechtsgrundlage ist Art. 6 Abs. 1 lit. f DSGVO; unser berechtigtes Interesse ist, deine Anfrage zu beantworten.",
        ],
    ),
    (
        "Cookies und lokaler Speicher",
        &[
            "Neben dem Sitzungs-Cookie „hashi_session“ merkt sich das Cookie „i18n_pref_locale“ deine Sprache, bis du den Browser schließt.",
            "Im lokalen Speicher deines Browsers liegen das Farbschema, die Ton- und Musikeinstellungen, deine laufenden Spiele, die Rätselwerkstatt, die Einstellungen des Solver-Labors und die Geräte-ID. Davon schickt dein Browser nur die Geräte-ID und das Zugprotokoll der Brücke des Tages an uns. Alles bleibt gespeichert, bis du es in deinem Browser löschst.",
            "Cookies und lokaler Speicher dienen nur dazu, die Funktionen bereitzustellen, die du nutzt (§ 25 Abs. 2 Nr. 2 TDDDG). Deshalb fragen wir nicht nach einer Einwilligung.",
        ],
    ),
    (
        "Schriftarten und Links",
        &[
            "Alle Schriftarten liegen auf unserem eigenen Server. Es werden keine Daten an Google Fonts oder andere Drittanbieter übermittelt.",
            "Die Knöpfe zum Teilen laden nichts von Drittanbietern. Erst wenn du auf WhatsApp oder X klickst, öffnet sich deren Seite mit deinem Ergebnis als Text; dort gelten die Datenschutzbestimmungen von Meta bzw. X. Dasselbe gilt für Links zu GitHub und zu Quellcode-Seiten im Dojo.",
        ],
    ),
    (
        "Speicherdauer",
        &[
            "Dein Konto, deine Ergebnisse, Stempel und Dojo-Einsendungen speichern wir, bis du dein Konto löschst. Das geht jederzeit auf deiner Kontoseite. Dabei löschen wir deinen Namen, dein Passwort, deine Stempel, Dojo-Einsendungen, Zugprotokolle, Geräte-Hashes und ungewerteten Ergebnisse. Deine gewerteten Zeiten bleiben ohne Namen in den Ranglisten vergangener Tage, damit die Plätze und Medaillen der anderen gleich bleiben; mit deiner Person lassen sie sich dann nicht mehr verbinden.",
            "Gastkonten, die ein Jahr lang nicht besucht wurden, löschen wir automatisch auf dieselbe Weise, denn ohne gültige Sitzung kommt niemand mehr an sie heran. Sitzungen laufen ein Jahr nach dem letzten Besuch ab, Kontaktnachrichten löschen wir spätestens nach sechs Monaten.",
        ],
    ),
    (
        "Deine Rechte",
        &[
            "Du hast das Recht auf Auskunft, Berichtigung, Löschung, Einschränkung der Verarbeitung und Datenübertragbarkeit (Art. 15–20 DSGVO) sowie auf Beschwerde bei einer Datenschutz-Aufsichtsbehörde (Art. 77 DSGVO), zum Beispiel der deines Wohnorts. Schreib uns dafür einfach eine E-Mail oder über das Kontaktformular: /kontakt",
            "Widerspruchsrecht: Soweit wir Daten auf Grundlage von Art. 6 Abs. 1 lit. f DSGVO verarbeiten, kannst du dem aus Gründen, die sich aus deiner besonderen Situation ergeben, jederzeit widersprechen (Art. 21 DSGVO).",
        ],
    ),
    ("Stand", &["Oktober 2026"]),
];

const PRIVACY_EN: &[Section] = &[
    (
        "Controller",
        &[
            "Noel Huibers, c/o Online-Impressum 12002, Europaring 90, 53757 Sankt Augustin, Germany",
            "Email: online-impressum.d50dw@passmail.net, contact form: /kontakt",
        ],
    ),
    (
        "In short",
        &[
            "Hashi has no ads, tracking or analytics. We only process the data the game needs and share it with nobody except the providers that run the site for us (Fly.io, Turso and Proton). Practice, the puzzle workshop and the solver lab need no account; puzzles you generate or solve there are processed only to answer the request and never stored.",
        ],
    ),
    (
        "Hosting and database",
        &[
            "The site is hosted by Fly.io, Inc. (USA) on servers in Amsterdam. Fly.io processes technically necessary data such as your IP address, time and page to deliver the site and protect it from attacks; our app itself keeps no access logs (Art. 6 (1) f GDPR). Fly.io is certified under the EU-U.S. Data Privacy Framework (Art. 45 GDPR).",
            "Accounts, results, stamps, Dojo submissions and contact messages are stored in a Turso database (ChiselStrike Inc., USA) in an Amazon Web Services data centre in Ireland, covered by the EU Standard Contractual Clauses (Art. 46 (2) c GDPR). Both providers act as our processors (Art. 28 GDPR).",
        ],
    ),
    (
        "Account, leaderboards and player profile",
        &[
            "Starting the Bridge of the Day creates a guest account with a random guest name and the cookie \"hashi_session\", which holds a random key we only store as a hash and expires a year after your last visit or when you log out. Registered accounts add a username and an Argon2 password hash. Failed logins are counted in memory for 15 minutes to slow down password guessing.",
            "For each daily we store your start and finish time, time, moves, hints, checks and solved board. The daily is a public competition: your name, whether you are a guest, your times, hints, checks, medals, streaks, monthly points, your stamps with their dates and your player profile (the last 35 days with time and place) are public. Player profiles are hidden from search engines. If you don't want to appear publicly, play the practice puzzles (Art. 6 (1) b GDPR).",
        ],
    ),
    (
        "Fair play",
        &[
            "To keep the leaderboard fair we store the log of your moves for each daily, and your browser keeps a random device ID in local storage that the server stores only as a keyed hash that changes every day, to see whether the same device already started the daily with another account. Results are checked automatically (impossibly fast solves, unnatural click pace, a second account on the same device) and may be reviewed by hand; flagged results don't appear on the leaderboard. If you think yours was flagged by mistake, write to us (Art. 6 (1) f GDPR, preventing cheating).",
        ],
    ),
    (
        "Stamps",
        &[
            "For the stamps we look at your results and move log, your monthly race, your Dojo entries and the age of your account. For the time-of-day stamps your browser sends its offset from UTC when you finish a daily; we don't store it, only whether you finished at night (midnight to 4 am) or over lunch (noon to 2 pm) in your local time (Art. 6 (1) b GDPR).",
        ],
    ),
    (
        "SAT Dojo",
        &[
            "Registered players can submit a solver as a WebAssembly file. We store the file, its name, description, source link, time and result; everything but the file itself is public along with your username. WebAssembly files can contain debug information such as local file paths, and we store the file as uploaded. Only the file, without anything about you, is sent to our own benchmark server, also at Fly.io in Amsterdam, and new entries are compared with earlier ones to detect copies (Art. 6 (1) b GDPR). To limit submissions per network we store a keyed hash of your IP address that changes every day, never the address itself (Art. 6 (1) f GDPR).",
        ],
    ),
    (
        "Contact form and email",
        &[
            "When you write to us through the contact form we store your message, your name and email address if you give them, and a keyed hash of your IP address that changes every day, to limit messages per network. The message is also delivered to us by email through Proton Mail (Proton AG, Route de la Galaise 32, 1228 Plan-les-Ouates, Switzerland) as our processor (Art. 28 GDPR); Switzerland has an EU adequacy decision (Art. 45 GDPR). The same applies when you email us directly. We only use it to answer you and delete stored messages once they're dealt with, after six months at the latest, and emails once we no longer need them (Art. 6 (1) f GDPR).",
        ],
    ),
    (
        "Cookies and local storage",
        &[
            "The cookie \"i18n_pref_locale\" remembers your language until you close the browser. Your colour scheme, sound and music settings, running games, puzzle workshop, solver lab settings and the device ID stay in your browser's local storage until you clear it; only the device ID and the daily's move log are sent to us. All of these are strictly necessary for the features you use (§ 25 (2) 2 TDDDG), so we don't ask for consent.",
        ],
    ),
    (
        "Fonts and links",
        &[
            "All fonts are served from our own server. The share buttons load nothing from third parties; only when you click WhatsApp or X does their site open with your result as text, and their privacy policies apply there. The same goes for links to GitHub and to source pages in the Dojo.",
        ],
    ),
    (
        "Retention",
        &[
            "We keep your account, results, stamps and Dojo submissions until you delete your account, which you can do at any time on your account page. That deletes your name, password, stamps, Dojo submissions, move logs, device hashes and unranked results; your ranked times stay on the leaderboards of past days without a name, so everyone else keeps their places and medals, and can no longer be connected to you. Guest accounts nobody has visited for a year are deleted the same way automatically, as nobody can get into them any more. Sessions expire a year after the last visit, contact messages are deleted after six months at the latest.",
        ],
    ),
    (
        "Your rights",
        &[
            "You have the right to access, rectification, erasure, restriction and portability (Art. 15–20 GDPR) and to lodge a complaint with a supervisory authority (Art. 77 GDPR). Where we rely on Art. 6 (1) f GDPR you can object at any time on grounds relating to your particular situation (Art. 21 GDPR). Just email us or use the contact form: /kontakt",
        ],
    ),
    ("Last updated", &["October 2026"]),
];

const LINK: &str = "text-primary underline underline-offset-4";

/// A paragraph with email addresses as mailto links and paths (`/kontakt`) as links in the
/// current language. Plain text between links stays one text node, for hydration.
fn paragraph(text: &'static str) -> impl IntoView {
    let i18n = use_i18n();
    let mut parts = Vec::new();
    let mut plain = String::new();
    for word in text.split_inclusive(' ') {
        let target = word.trim_end_matches([' ', '.', ',']);
        let tail = &word[target.len()..];
        let link = if target.contains('@') {
            view! { <a href=format!("mailto:{target}") class=LINK>{target}</a> }.into_any()
        } else if target.len() > 1 && target.starts_with('/') {
            let href = move || localized(i18n.get_locale(), target);
            view! { <A href=href attr:class=LINK>{format!("hashi.rs{target}")}</A> }.into_any()
        } else {
            plain.push_str(word);
            continue;
        };
        if !plain.is_empty() {
            parts.push(std::mem::take(&mut plain).into_any());
        }
        parts.push(link);
        plain.push_str(tail);
    }
    if !plain.is_empty() {
        parts.push(plain.into_any());
    }
    view! { <p class="leading-relaxed text-muted-foreground">{parts}</p> }
}

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
                        .map(|p| paragraph(p))
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
