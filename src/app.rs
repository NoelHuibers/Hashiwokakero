use leptos::prelude::*;
use leptos_meta::{HashedStylesheet, Meta, MetaTags, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::components::layout::{Footer, Header, Toaster};
use crate::pages::NotFound;
use crate::pages::create::CreatePage;
use crate::pages::lab::LabPage;
use crate::pages::play::PlayPage;
use crate::pages::rules::RulesPage;
use crate::state::provide_state;

/// Applies the saved (or system) colour scheme before the first paint, so night mode doesn't flash.
const THEME_SCRIPT: &str = "try{var t=localStorage.getItem('hashi.theme');if(t==='dark'||(!t&&matchMedia('(prefers-color-scheme: dark)').matches))document.documentElement.classList.add('dark')}catch(e){}";

const FONTS: &str = "https://fonts.googleapis.com/css2?family=DotGothic16&family=Zen+Maru+Gothic:wght@400;500;700;900&display=swap";

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="author" content="Noel Huibers" />
                <meta name="color-scheme" content="light dark" />
                <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
                <link rel="icon" href="/favicon.ico" sizes="32x32" />
                <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
                <link rel="manifest" href="/manifest.webmanifest" />
                <script inner_html=THEME_SCRIPT></script>
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="" />
                <link rel="stylesheet" href=FONTS />
                // Rendered here and not in <App/>: the hashed file name is only known on the server
                <HashedStylesheet options=options.clone() id="leptos" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_state();

    view! {
        // Title, description and social tags are set per page, see seo::PageMeta
        <Meta name="theme-color" content="#cf3a24" />

        <Router>
            <div class="flex min-h-screen flex-col">
                <Header />
                <main class="flex-1">
                    <Routes fallback=|| view! { <NotFound /> }>
                        <Route path=path!("") view=PlayPage />
                        <Route path=path!("/create") view=CreatePage />
                        <Route path=path!("/lab") view=LabPage />
                        <Route path=path!("/rules") view=RulesPage />
                    </Routes>
                </main>
                <Footer />
            </div>
            <Toaster />
        </Router>
    }
}
