//! The feedback form: bugs, ideas, questions and anything else, also the second way to reach us
//! next to email (the Impressum needs one).
//!
//! `/kontakt?type=bug&from=/practice`: the links to it (footer, the header's help, the account
//! page) pick the type and pass the page they were on. What the page sends along with the
//! message (`FeedbackContext`) is listed above the button.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::api::send_message;
use crate::components::layout::use_localized;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::components::ui::toggle_group::{ToggleGroup, ToggleGroupItem};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{error_text, localized, unlocalized};
use crate::model::{ContextField, FeedbackContext, FeedbackKind, daily_number, user_agent_parts};
use crate::pages::legal::CONTACT_EMAIL;
use crate::seo::PageMeta;
use crate::state::{Auth, recent_errors};

const MAX_MESSAGE: usize = 5000;

/// The form in `locale`, of `kind` if given, opened from the page at `from` (with or without its
/// language prefix; not the form itself).
pub fn feedback_link(locale: Locale, kind: Option<FeedbackKind>, from: &str) -> String {
    let from = unlocalized(from);
    let mut query = Vec::new();
    if let Some(kind) = kind {
        query.push(format!("type={}", kind.id()));
    }
    if from.starts_with('/') && from != "/kontakt" {
        query.push(format!("from={}", encode_component(&from)));
    }
    let path = localized(locale, "/kontakt");
    if query.is_empty() {
        path
    } else {
        format!("{path}?{}", query.join("&"))
    }
}

/// `text` for a URL's query or a `mailto:` link: everything but letters, digits, `-_.~/`
/// percent-encoded, as UTF-8.
pub fn encode_component(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// What only the browser knows of the context: the app, the user agent's system and browser,
/// the window's size.
#[derive(Clone, Debug, PartialEq)]
struct Device {
    app: Option<(&'static str, &'static str)>,
    os: String,
    browser: String,
    screen: String,
    errors: Vec<String>,
}

impl Device {
    fn here() -> Self {
        let window = window();
        let user_agent = window.navigator().user_agent().unwrap_or_default();
        let (os, browser) = user_agent_parts(&user_agent);
        let size =
            |v: Result<wasm_bindgen::JsValue, _>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        Self {
            app: crate::api::app_identity(),
            os,
            browser,
            screen: screen(
                size(window.inner_width()),
                size(window.inner_height()),
                window.device_pixel_ratio(),
            ),
            errors: recent_errors(),
        }
    }
}

/// `390×844 @3x`, `1280×720 @1.25x`.
fn screen(width: f64, height: f64, ratio: f64) -> String {
    let ratio = format!("{ratio:.2}");
    let ratio = ratio.trim_end_matches('0').trim_end_matches('.');
    format!("{width:.0}×{height:.0} @{ratio}x")
}

/// The context of a message: the device's, the page's language, the page it came from, today's
/// daily and the player.
fn context(
    device: &Device,
    locale: Locale,
    from: Option<String>,
    player: Option<i64>,
) -> FeedbackContext {
    use leptos_i18n::Locale as _;
    FeedbackContext {
        app: device.app.map(|(_, version)| version.to_string()),
        platform: device
            .app
            .map_or("web", |(platform, _)| platform)
            .to_string(),
        os: device.os.clone(),
        browser: device.browser.clone(),
        language: locale.as_str().to_string(),
        page: from,
        daily: Some(daily_number(crate::seo::today())),
        player,
        screen: device.screen.clone(),
        errors: device.errors.clone(),
    }
}

/// The name of a line of the context.
fn field_label(locale: Locale, field: ContextField) -> &'static str {
    match field {
        ContextField::App => td_string!(locale, contact.context.app),
        ContextField::Platform => td_string!(locale, contact.context.platform),
        ContextField::System => td_string!(locale, contact.context.system),
        ContextField::Browser => td_string!(locale, contact.context.browser),
        ContextField::Language => td_string!(locale, contact.context.language),
        ContextField::Page => td_string!(locale, contact.context.page),
        ContextField::Daily => td_string!(locale, contact.context.daily),
        ContextField::Player => td_string!(locale, contact.context.player),
        ContextField::Screen => td_string!(locale, contact.context.screen),
        ContextField::Errors => td_string!(locale, contact.context.errors),
    }
}

/// A kind's name, and what it is for.
fn kind_texts(locale: Locale, kind: FeedbackKind) -> (&'static str, &'static str) {
    match kind {
        FeedbackKind::Bug => (
            td_string!(locale, contact.kinds.bug),
            td_string!(locale, contact.kind_hints.bug),
        ),
        FeedbackKind::Idea => (
            td_string!(locale, contact.kinds.idea),
            td_string!(locale, contact.kind_hints.idea),
        ),
        FeedbackKind::Question => (
            td_string!(locale, contact.kinds.question),
            td_string!(locale, contact.kind_hints.question),
        ),
        FeedbackKind::Other => (
            td_string!(locale, contact.kinds.other),
            td_string!(locale, contact.kind_hints.other),
        ),
    }
}

fn placeholder(locale: Locale, kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Bug => td_string!(locale, contact.placeholders.bug),
        FeedbackKind::Idea => td_string!(locale, contact.placeholders.idea),
        FeedbackKind::Question => td_string!(locale, contact.placeholders.question),
        FeedbackKind::Other => td_string!(locale, contact.placeholders.other),
    }
}

/// The lines of the context as the form lists them: the name, and the value (the version as
/// `App 1.1.0` or `Website`). Errors come under one name.
fn context_rows(locale: Locale, context: &FeedbackContext) -> Vec<(&'static str, Vec<String>)> {
    let mut rows: Vec<(&'static str, Vec<String>)> = Vec::new();
    for (field, value) in context.rows() {
        let value = match field {
            ContextField::App if value.is_empty() => {
                td_string!(locale, contact.context.website).to_string()
            }
            ContextField::App => {
                td_string!(locale, contact.context.app_version, version = value).to_string()
            }
            _ => value,
        };
        let label = field_label(locale, field);
        match rows.last_mut() {
            Some((last, values)) if *last == label => values.push(value),
            _ => rows.push((label, vec![value])),
        }
    }
    rows
}

#[component]
pub fn ContactPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let privacy = use_localized("/datenschutz");
    let query = use_query_map();
    let kind = RwSignal::new(
        query
            .with_untracked(|q| q.get("type"))
            .and_then(|t| FeedbackKind::from_id(&t))
            .unwrap_or_default(),
    );
    let from = query
        .with_untracked(|q| q.get("from"))
        .map(|f| unlocalized(&f))
        .filter(|f| f.starts_with('/') && !f.starts_with("//"));
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    // Hidden from people, bots fill it in
    let website = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let sent = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    // Only in the browser, once the page runs there
    let device = RwSignal::new(None::<Device>);
    Effect::new(move |_| device.set(Some(Device::here())));
    let context = Memo::new(move |_| {
        let player = auth.user.with(|u| u.as_ref().map(|u| u.id));
        device.with(|d| {
            d.as_ref()
                .map(|d| context(d, i18n.get_locale(), from.clone(), player))
        })
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let fields = (
            name.get_untracked(),
            email.get_untracked(),
            message.get_untracked(),
            website.get_untracked(),
            kind.get_untracked(),
            context.get_untracked(),
        );
        spawn_local(async move {
            match send_message(fields.0, fields.1, fields.2, fields.3, fields.4, fields.5).await {
                Ok(()) => {
                    message.set(String::new());
                    sent.set(true);
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };
    let optional = move || format!("({})", t_string!(i18n, contact.optional));

    view! {
        <PageMeta
            title=Signal::derive(move || format!("{} · Hashi", t_string!(i18n, contact.title)))
            description=Signal::derive(move || t_string!(i18n, contact.description).to_string())
            path="/kontakt"
        />
        <section class="mx-auto flex max-w-2xl flex-col gap-6 px-4 py-8 sm:px-6">
            <div>
                <p class="font-pixel text-sm text-primary">"意見 · iken"</p>
                <h1 class="text-3xl font-black">{t!(i18n, contact.title)}</h1>
                <p class="mt-2 text-muted-foreground">{t!(i18n, contact.intro)}</p>
            </div>
            <Card>
                <CardContent>
                    <Show
                        when=move || !sent.get()
                        fallback=move || {
                            view! {
                                <div class="flex flex-col items-start gap-3 py-2">
                                    <p class="text-xl font-bold">{t!(i18n, contact.sent_title)} " ありがとう"</p>
                                    <p class="text-muted-foreground">{t!(i18n, contact.sent)}</p>
                                    <Button variant=ButtonVariant::Outline on:click=move |_| {
                                        // The errors since, if there were any
                                        device.set(Some(Device::here()));
                                        sent.set(false);
                                    }>
                                        {t!(i18n, contact.another)}
                                    </Button>
                                </div>
                            }
                        }
                    >
                        <form class="flex flex-col gap-4" on:submit=submit>
                            <div class="flex flex-col gap-2">
                                <p class="text-sm font-medium">{t!(i18n, contact.kind)}</p>
                                <ToggleGroup
                                    aria_label=Signal::derive(move || t_string!(i18n, contact.kind).to_string())
                                    class="grid w-full grid-cols-2 rounded-lg border bg-muted/60 p-1 sm:grid-cols-4"
                                >
                                    {FeedbackKind::ALL
                                        .into_iter()
                                        .map(|k| {
                                            view! {
                                                <ToggleGroupItem
                                                    title=Signal::derive(move || kind_texts(i18n.get_locale(), k).1.to_string())
                                                    pressed=Signal::derive(move || kind.get() == k)
                                                    class="font-semibold data-[state=on]:bg-primary data-[state=on]:text-primary-foreground"
                                                    on:click=move |_| kind.set(k)
                                                >
                                                    {move || kind_texts(i18n.get_locale(), k).0}
                                                </ToggleGroupItem>
                                            }
                                        })
                                        .collect_view()}
                                </ToggleGroup>
                                <p class="text-xs text-muted-foreground">{move || kind_texts(i18n.get_locale(), kind.get()).1}</p>
                            </div>
                            <div class="flex flex-col gap-2">
                                <Label html_for="contact-message">{t!(i18n, contact.message)}</Label>
                                <textarea
                                    id="contact-message"
                                    class="min-h-40 w-full rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 dark:bg-input/30 md:text-sm"
                                    placeholder=move || placeholder(i18n.get_locale(), kind.get())
                                    bind:value=message
                                    required
                                    minlength=10
                                    maxlength=MAX_MESSAGE
                                ></textarea>
                                <p class="self-end text-xs tabular-nums text-muted-foreground">
                                    {move || format!("{} / {MAX_MESSAGE}", message.with(|m| m.chars().count()))}
                                </p>
                            </div>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <div class="flex flex-col gap-2">
                                    <Label html_for="contact-name">
                                        {t!(i18n, contact.name)} " "
                                        <span class="font-normal text-muted-foreground">{optional}</span>
                                    </Label>
                                    <Input id="contact-name" bind_value=name autocomplete="name" attr:maxlength="80" />
                                </div>
                                <div class="flex flex-col gap-2">
                                    <Label html_for="contact-email">
                                        {t!(i18n, contact.email)} " "
                                        <span class="font-normal text-muted-foreground">{optional}</span>
                                    </Label>
                                    <Input id="contact-email" r#type=InputType::Email bind_value=email autocomplete="email" attr:maxlength="254" />
                                    <p class="text-xs text-muted-foreground">{t!(i18n, contact.email_hint)}</p>
                                </div>
                            </div>
                            // The honeypot: off screen and skipped by keyboards and screen readers. Off the
                            // start side: past the end, right to left, the page would scroll to it
                            <div class="absolute -start-[9999px] size-px overflow-hidden" aria-hidden="true">
                                <label for="contact-website">"Website"</label>
                                <input id="contact-website" type="text" tabindex="-1" autocomplete="off" bind:value=website />
                            </div>
                            <details class="group rounded-lg border bg-muted/40 px-3 py-2 text-sm" data-testid="feedback-context">
                                <summary class="cursor-pointer font-medium marker:text-muted-foreground">
                                    {t!(i18n, contact.context.title)}
                                </summary>
                                <p class="mt-2 text-xs text-muted-foreground">{t!(i18n, contact.context.intro)}</p>
                                <dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-xs">
                                    {move || {
                                        context
                                            .get()
                                            .map(|c| {
                                                context_rows(i18n.get_locale(), &c)
                                                    .into_iter()
                                                    .map(|(label, values)| {
                                                        view! {
                                                            <dt class="text-muted-foreground">{label}</dt>
                                                            <dd class="min-w-0 break-words font-mono">
                                                                {values.into_iter().map(|v| view! { <span class="block">{v}</span> }).collect_view()}
                                                            </dd>
                                                        }
                                                    })
                                                    .collect_view()
                                            })
                                    }}
                                </dl>
                            </details>
                            {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                            <Button attr:r#type="submit" attr:disabled=move || busy.get() class="w-full sm:w-auto sm:self-start">
                                {move || if busy.get() { t_string!(i18n, contact.sending).to_string() } else { format!("{} 送る", t_string!(i18n, contact.send)) }}
                            </Button>
                            <p class="text-xs text-muted-foreground">
                                {t!(
                                    i18n,
                                    contact.privacy,
                                    <a> = move |children: ChildrenFn| view! {
                                        <A href=privacy attr:class="text-primary underline underline-offset-4">{children()}</A>
                                    }
                                )}
                            </p>
                        </form>
                    </Show>
                </CardContent>
            </Card>
            <p class="text-sm text-muted-foreground">
                {t!(i18n, contact.by_mail)} " "
                <a class="text-primary underline underline-offset-4" href=format!("mailto:{CONTACT_EMAIL}")>
                    {CONTACT_EMAIL}
                </a>
            </p>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The links to the form pick the type and pass the page, in its language and without its
    /// prefix; the form itself isn't a page it came from.
    #[test]
    fn links_say_where_they_came_from() {
        assert_eq!(feedback_link(Locale::en, None, "/"), "/kontakt?from=/");
        assert_eq!(
            feedback_link(Locale::de, Some(FeedbackKind::Bug), "/de/practice"),
            "/de/kontakt?type=bug&from=/practice"
        );
        assert_eq!(
            feedback_link(Locale::ja, Some(FeedbackKind::Question), "/ja/kontakt"),
            "/ja/kontakt?type=question"
        );
        assert_eq!(feedback_link(Locale::en, None, "/kontakt"), "/kontakt");
        assert_eq!(
            feedback_link(Locale::en, Some(FeedbackKind::Idea), "/player/7"),
            "/kontakt?type=idea&from=/player/7"
        );
        assert_eq!(
            encode_component("Re: #3 & ä?\n"),
            "Re%3A%20%233%20%26%20%C3%A4%3F%0A"
        );
    }

    #[test]
    fn screens_say_their_size_and_pixel_ratio() {
        assert_eq!(screen(390.0, 844.0, 3.0), "390×844 @3x");
        assert_eq!(screen(1280.4, 719.6, 1.25), "1280×720 @1.25x");
        assert_eq!(screen(412.0, 915.0, 2.75), "412×915 @2.75x");
        assert_eq!(screen(800.0, 600.0, 1.5), "800×600 @1.5x");
    }

    /// What the form lists is what it sends: the device's details, the language, the page, the
    /// daily and the player; on the website without a version.
    #[test]
    fn the_listed_context_is_the_sent_one() {
        let device = Device {
            app: Some(("android", "1.1.0")),
            os: "Android 15".into(),
            browser: "WebView 141".into(),
            screen: "390×844 @3x".into(),
            errors: vec!["/ E:database".into(), "/ap E:internal".into()],
        };
        let sent = context(&device, Locale::de, Some("/practice".into()), Some(7));
        assert_eq!(
            (
                sent.app.as_deref(),
                sent.platform.as_str(),
                sent.language.as_str()
            ),
            (Some("1.1.0"), "android", "de")
        );
        assert_eq!(
            (sent.page.as_deref(), sent.player),
            (Some("/practice"), Some(7))
        );
        assert!(sent.daily.is_some_and(|d| d > 0));
        let rows = context_rows(Locale::de, &sent);
        assert_eq!(rows[0], ("Version", vec!["App 1.1.0".to_string()]));
        assert!(rows.contains(&("Seite", vec!["/practice".to_string()])));
        assert!(rows.contains(&("Spieler", vec!["#7".to_string()])));
        // The errors under one name
        assert_eq!(
            rows.last(),
            Some(&(
                "Letzte Fehler",
                vec!["/ E:database".to_string(), "/ap E:internal".to_string()]
            ))
        );

        let website = Device {
            app: None,
            errors: Vec::new(),
            ..device
        };
        let sent = context(&website, Locale::en, None, None);
        assert_eq!((sent.app.as_deref(), sent.platform.as_str()), (None, "web"));
        let rows = context_rows(Locale::en, &sent);
        assert_eq!(rows[0], ("Version", vec!["Website".to_string()]));
        assert!(
            rows.iter()
                .all(|(label, _)| !["Page", "Player", "Recent errors"].contains(label))
        );
    }

    /// Every type has its name, hint and placeholder in every language.
    #[test]
    fn every_kind_is_explained() {
        use leptos_i18n::Locale as _;
        for &locale in Locale::get_all() {
            for kind in FeedbackKind::ALL {
                let (name, hint) = kind_texts(locale, kind);
                assert!(
                    !name.is_empty() && !hint.is_empty() && !placeholder(locale, kind).is_empty()
                );
            }
        }
    }
}
