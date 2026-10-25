//! The contact form, the second way to reach us next to email (the Impressum needs one).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::send_message;
use crate::components::layout::use_localized;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::error_text;
use crate::pages::legal::CONTACT_EMAIL;
use crate::seo::PageMeta;

const MAX_MESSAGE: usize = 5000;

#[component]
pub fn ContactPage() -> impl IntoView {
    let i18n = use_i18n();
    let privacy = use_localized("/datenschutz");
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    // Hidden from people, bots fill it in
    let website = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let sent = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

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
        );
        spawn_local(async move {
            match send_message(fields.0, fields.1, fields.2, fields.3).await {
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
                <p class="font-pixel text-sm text-primary">"連絡 · renraku"</p>
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
                                    <Button variant=ButtonVariant::Outline on:click=move |_| sent.set(false)>
                                        {t!(i18n, contact.another)}
                                    </Button>
                                </div>
                            }
                        }
                    >
                        <form class="flex flex-col gap-4" on:submit=submit>
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
                            <div class="flex flex-col gap-2">
                                <Label html_for="contact-message">{t!(i18n, contact.message)}</Label>
                                <textarea
                                    id="contact-message"
                                    class="min-h-40 w-full rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 dark:bg-input/30 md:text-sm"
                                    bind:value=message
                                    required
                                    minlength=10
                                    maxlength=MAX_MESSAGE
                                ></textarea>
                                <p class="self-end text-xs tabular-nums text-muted-foreground">
                                    {move || format!("{} / {MAX_MESSAGE}", message.with(|m| m.chars().count()))}
                                </p>
                            </div>
                            // The honeypot: off screen and skipped by keyboards and screen readers. Off the
                            // start side: past the end, right to left, the page would scroll to it
                            <div class="absolute -start-[9999px] size-px overflow-hidden" aria-hidden="true">
                                <label for="contact-website">"Website"</label>
                                <input id="contact-website" type="text" tabindex="-1" autocomplete="off" bind:value=website />
                            </div>
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
