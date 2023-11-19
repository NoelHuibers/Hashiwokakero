use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/leptos_start.css"/>

        // sets the document title
        <Title text="Hashiwakakeru"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes>
                    <Route path="" view=HomePage/>
                    <Route path="/*any" view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    let (rows, set_rows) = create_signal(3);
    let (columns, set_columns) = create_signal(3);

    view! {
        <main class="h-full w-full min-h-screen min-w-full flex flex-col items-center space-y-4 bg-slate-100 p-16">
            <h1 class="font-bold text-4xl text-violet-700">"Hashiwakakeru"</h1>
            <h2 class="font-bold text-xl text-violet-600">"Bridges Game"</h2>
            <div class="flex flex-row space-x-4 items-center">
                <p>"Rows:"</p>
                <input
                    class="bg-slate-100 border rounded p-2 text-violet-700 w-16"
                    type="number"
                    min="3"
                    prop:value=rows
                    on:input=move |rows| {
                        set_rows(event_target_value(&rows).parse().unwrap_or(3));
                    }
                />

                <p>"Columns:"</p>
                <input
                    class="bg-slate-100 border rounded p-2 text-violet-700 w-16"
                    type="number"
                    min="3"
                    prop:value=columns
                    on:input=move |columns| {
                        set_columns(event_target_value(&columns).parse().unwrap_or(3));
                    }
                />

            </div>
            <div class="h-fit w-fit items-center">
                <Table rows=rows columns=columns/>
            </div>
            <div class="flex flex-row space-x-4">
                <button class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100">
                    "To CNF"
                </button>
                <button class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100">"Solve"</button>
                <button class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100">
                    "Generate"
                </button>
                <button class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100">"Clear"</button>
            </div>
        </main>
    }
}

#[component]
fn Table(rows: ReadSignal<i32>, columns: ReadSignal<i32>) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center">
            <For
                each=move || (0..rows.get()).into_iter()
                key=|counter| *counter
                children=move |id| {
                    view! {
                        <div>
                            <Column columns=columns.clone()/>
                        </div>
                    }
                }
            />

        </div>
    }
}

#[component]
fn Column(columns: ReadSignal<i32>) -> impl IntoView {
    view! {
        <div class="flex flex-row items-center">
            <For
                each=move || (0..columns.get()).into_iter()
                key=|counter| *counter
                children=move |id| {
                    view! {
                        <input
                            type="text"
                            min="0"
                            max="8"
                            class="h-16 w-16 border bg-violet-700 p-2 justify-center text-slate-100 text-lg"
                        />
                    }
                }
            />

        </div>
    }
}
/// 404 - Not Found
#[component]
fn NotFound() -> impl IntoView {
    // set an HTTP status code 404
    // this is feature gated because it can only be done during
    // initial server-side rendering
    // if you navigate to the 404 page subsequently, the status
    // code will not be set because there is not a new HTTP request
    // to the server
    #[cfg(feature = "ssr")]
    {
        // this can be done inline because it's synchronous
        // if it were async, we'd use a server function
        let resp = expect_context::<leptos_actix::ResponseOptions>();
        resp.set_status(actix_web::http::StatusCode::NOT_FOUND);
    }

    view! { <h1>"Not Found"</h1> }
}

