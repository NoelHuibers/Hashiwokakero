use leptos::{ev::SubmitEvent, *, html::Input};
use leptos_meta::*;
use leptos_router::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Event, HtmlFormElement};

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

    let (tableprop, set_tableprop) = create_signal(vec![vec![0; 3]; 3]);
    create_effect(move |_| {
        set_tableprop(vec![vec![0; columns.get() as usize]; rows.get() as usize]);
    });

    let file_input: NodeRef<Input> = create_node_ref();
    let filechanged = move |ev: Event| {
        if let Some(files) = file_input.get().and_then(|f: HtmlElement<Input>|
            f.files()) {
                let file = files.get(0).unwrap();
                let reader = web_sys::FileReader::new().unwrap();
                let onload = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let reader = event.target().unwrap().dyn_into::<web_sys::FileReader>().unwrap();
                    let result = reader.result().unwrap().as_string().unwrap();
                    let mut lines = result.lines();
                    let mut first_line = lines.next().unwrap().split_whitespace();
                    let rows = first_line.next().unwrap().parse::<i32>().unwrap();
                    let columns = first_line.next().unwrap().parse::<i32>().unwrap();
                    set_rows(rows);
                    set_columns(columns);
                    let mut table = vec![vec![0; columns as usize]; rows as usize];
                    for (i, line) in lines.enumerate() {
                        let numbers = line.split_whitespace();
                        for (j, number) in numbers.enumerate() {
                            table[i][j] = number.parse::<i32>().unwrap();
                        }
                    }
                    set_tableprop(table);
                }) as Box<dyn FnMut(_)>);
                reader.add_event_listener_with_callback("loadend", onload.as_ref().unchecked_ref()).unwrap();
                onload.forget();

                reader.read_as_text(&file).unwrap();
            }
    };

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
                <Table
                    rows=rows
                    columns=columns
                    tableprop=tableprop.clone()
                    set_tableprop=set_tableprop.clone()
                />
            </div>
            <input
                name="Upload"
                type="file"
                accept=".txt"
                node_ref=file_input
                on:change=move |ev| {
                    filechanged(ev);
                }
            />

        </main>
    }
}

#[component]
fn Table(
    rows: ReadSignal<i32>,
    columns: ReadSignal<i32>,
    tableprop: ReadSignal<Vec<Vec<i32>>>,
    set_tableprop: WriteSignal<Vec<Vec<i32>>>,
) -> impl IntoView {
    let (testprop, set_testprop) = create_signal("0".to_string());
    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let submitid = ev.submitter().unwrap().id();
        // if submitid == "0" {         async move {
        //     let res = to_cnf(tableprop.get()).await;
        //     };
        // }
        if submitid == "1" {
                // spawn_local(async {
                //     match solvepuzzle(tableprop.get()).await {
                //         Ok(result) => {
                //             set_testprop(result);
                //         }
                //         Err(error) => {
                //             set_testprop("Error".to_string());
                //         }
                //     }
                // });
        } else if submitid == "2" {
            // Generate
        } else if submitid == "3" {
            let form = ev
                .target()
                .map(|target| target.dyn_into::<HtmlFormElement>().ok())
                .flatten();
            if let Some(form) = form {
                form.reset();
                set_tableprop(vec![vec![0; columns.get() as usize]; rows.get() as usize]);
            }
        }
    };
    view! {
        <form on:submit=on_submit class="flex flex-col items-center">
            <For
                each=move || (0..rows.get()).into_iter()
                key=|counter| *counter
                children=move |id| {
                    view! {
                        <Column
                            columns=columns
                            tableprop=tableprop
                            set_tableprop=set_tableprop
                            rowid=id
                        />
                    }
                }
            />

            <div class="flex flex-row space-x-4 py-8">
                <input
                    type="submit"
                    id="0"
                    value="To CNF"
                    class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                />
                <input
                    type="submit"
                    id="1"
                    value="Solve"
                    class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                />
                <input
                    type="submit"
                    id="2"
                    value="Generate"
                    class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                />
                <input
                    type="submit"
                    id="3"
                    value="Clear"
                    class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                />
            </div>
        </form>
        <p>{testprop}</p>
    }
}

#[component]
fn Column(
    columns: ReadSignal<i32>,
    tableprop: ReadSignal<Vec<Vec<i32>>>,
    set_tableprop: WriteSignal<Vec<Vec<i32>>>,
    rowid: i32,
) -> impl IntoView {
    view! {
        <div class="flex flex-row items-center">
            <For
                each=move || (0..columns.get()).into_iter()
                key=|counter| *counter
                children=move |id| {
                    view! {
                        <input
                            type="text"
                            value=move || tableprop.get()[id as usize][rowid as usize].to_string()
                            min="0"
                            max="8"
                            class="h-16 w-16 border bg-violet-700 justify-center p-2 text-slate-100 text-lg"
                            on:input=move |value| {
                                let mut table = tableprop.get();
                                table[id
                                    as usize][rowid
                                    as usize] = event_target_value(&value).parse().unwrap_or(0);
                                set_tableprop(table);
                            }
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

use backend::parse_input::parse_vec_input;
use backend::solver::solve;
use backend::solver::get_content;
use backend::generate_clauses::generate;
use backend::writer::generate_dimacs;
use backend::reconstruct::reconstruct_puzzle;
#[server(Solve, "/solve")]
pub async fn solvepuzzle(puzzle: Vec<Vec<i32>>) -> Result<String, ServerFnError> {
    let game_board = parse_vec_input(puzzle);
    match game_board {
        Ok(game_board) => {
            let (clauses, var_map) = generate(&game_board);
            let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), "output.cnf");
            match dimacs_generated {
                Ok(_) => {
                    match solve("output.cnf") {
                        Ok(certificate) => {
                            let contents = get_content(certificate);
                            let res = reconstruct_puzzle(
                                contents,
                                &var_map,
                                &game_board);
                            Ok(res)
                        }
                        Err(e) => Err(ServerFnError::ServerError(e.to_string()))
                    }
                }
                Err(e) => Err(ServerFnError::ServerError(e.to_string()))
            }
        }
        Err(e) => Err(ServerFnError::ServerError(e.to_string()))
    }
}

#[server(ToCNF, "/to_cnf")]
pub async fn to_cnf(puzzle: Vec<Vec<i32>>) -> Result<(), ServerFnError> {
    let game_board = parse_vec_input(puzzle);
    match game_board {
        Ok(game_board) => {
            let (clauses, var_map) = generate(&game_board);
            let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), "output.cnf");
            Ok(dimacs_generated?)
        }
        _ => {Ok(())}
    }
}

#[server(Generate, "/generate")]
pub async fn generatfield(rows: i32, columns: i32) -> Result<(), ServerFnError> {
    //let game = generator(rows, columns);
    Ok(())
}



































































