use backend::generator::generator;
use js_sys::Array;
use leptos::{*, html::Input};
use serde::{Serialize,Deserialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::{Event, Blob, Url, MouseEvent};
use crate::error_template::{AppError, ErrorTemplate};
use leptos_meta::*;
use leptos_router::*;
use serde_json::json;

#[derive(Serialize, Deserialize)]
struct FullTable(Vec<Vec<u8>>);

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/tailwind.css"/>

        // sets the document title
        <Title text="Hashiwakakeru"/>

        <Router fallback=|| {
            let mut outside_errors = Errors::default();
            outside_errors.insert_with_default_key(AppError::NotFound);
            view! { <ErrorTemplate outside_errors/> }.into_view()
        }>
            <main>
                <Routes>
                    <Route path="" view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    let (rows, set_rows) = create_signal(3 as usize);
    let (columns, set_columns) = create_signal(3 as usize);

    let (tableprop, set_tableprop) = create_signal(vec![vec![0; 3]; 3]);
    create_effect(move |_| {
        set_tableprop(vec![vec![0; rows.get()]; columns.get()]);
    });

    let (json, set_json) = create_signal(json!(vec![vec![0; 3]; 3]).to_string());
    create_effect(move |_| {
        set_json(json!(tableprop.get()).to_string());
    });

    let file_input: NodeRef<Input> = create_node_ref();
    let filechanged = move |_ev: Event| {
        if let Some(files) = file_input.get().and_then(|f: HtmlElement<Input>|
            f.files()) {
                let file = files.get(0).unwrap();
                let reader = web_sys::FileReader::new().unwrap();
                let onload = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let reader = event.target().unwrap().dyn_into::<web_sys::FileReader>().unwrap();
                    let result = reader.result().unwrap().as_string().unwrap();
                    let mut lines = result.lines();
                    let mut first_line = lines.next().unwrap().split_whitespace();
                    let rows = first_line.next().unwrap().parse::<usize>().unwrap();
                    let columns = first_line.next().unwrap().parse::<usize>().unwrap();
                    let mut table = vec![vec![0; columns]; rows];
                    set_rows(rows);
                    set_columns(columns);
                    for (i, line) in lines.enumerate() {
                        for (j, number) in line.chars().enumerate() {
                            if number != '.' {
                                table[j][i] = number.to_digit(10).unwrap() as u8;
                            } else {
                                table[j][i] = 0;
                            }
                        }
                    }
                    set_tableprop(table);
                }) as Box<dyn FnMut(_)>);
                reader.add_event_listener_with_callback("loadend", onload.as_ref().unchecked_ref()).unwrap();
                onload.forget();

                reader.read_as_text(&file).unwrap();
            }
    };

    let (solved, set_solved) = create_signal(vec!["".to_string()]);
    let to_cnf = create_server_action::<ToCNF>();
    let to_cnfvalues = to_cnf.value();
    create_effect(move |_| {
        if let Some(grid) = to_cnfvalues(){
            match grid {
                Ok(result) => {
                    let data = Array::of1(&JsValue::from_str(&result));
                    let blob = Blob::new_with_str_sequence(&data).unwrap();
                    let url = Url::create_object_url_with_blob(&blob).unwrap();
                    let link = web_sys::window().unwrap().document().unwrap().create_element("a").unwrap();
                    link.set_attribute("href", &url).unwrap();
                    link.set_attribute("download", "output.txt").unwrap();
                    let click_event = MouseEvent::new("click").unwrap();
                    link.dispatch_event(&click_event).unwrap();
                    Url::revoke_object_url(&url).unwrap();
            }
                Err(_) => {
                    return;
            }
        };
    }});
    
    let generate = create_server_action::<Generate>();
    let generatevalues = generate.value();
    create_effect(move |_| {
        if let Some(grid) = generatevalues(){
            match grid {
                Ok(result) => {
                    set_tableprop(result);
            }
                Err(_) => {
                    return;
            }
        };
    }});

    let solve = create_server_action::<Solve>();
    let solvevalues = solve.value();
    create_effect(move |_| {
        if let Some(grid) = solvevalues(){
            match grid {
                Ok(result) => {
                    set_solved(result.split("\n").map(|s| s.to_string()).collect::<Vec<_>>());
            }
                Err(_) => {
                    return;
            }
        };
    }});

    view! {
        <main class="h-full w-full min-h-screen min-w-full flex flex-col items-center space-y-4 bg-slate-100 p-16">
            <h1 class="font-bold text-4xl text-violet-700">"Hashiwakakeru"</h1>
            <h2 class="font-bold text-xl text-violet-600">"Bridges Game"</h2>
            <div class="flex flex-col items-center space-y-4">
                <Show when=move || solved.get() == vec![""]>
                    <div class="flex flex-row space-x-4 items-center">
                        <p>"Rows:"</p>
                        <input
                            class="bg-slate-100 border rounded p-2 text-violet-700 w-16"
                            type="number"
                            name="rows"
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
                            name="columns"
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
                    <div class="flex flex-row space-x-4 py-8">
                        <ActionForm action=solve>
                            <input
                                type="submit"
                                value="Solve"
                                class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                            />
                            <input type="hidden" name="tableprop" prop:value=json/>
                            <input type="hidden" name="rows" prop:value=rows/>
                        </ActionForm>
                        <ActionForm action=to_cnf>
                            <input
                                type="submit"
                                value="To CNF"
                                class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                            />
                            <input type="hidden" name="tableprop" prop:value=json/>
                            <input type="hidden" name="rows" prop:value=rows/>
                        </ActionForm>
                        <ActionForm action=generate>
                            <input
                                type="submit"
                                value="Generate"
                                class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                            />
                            <input type="hidden" name="rows" prop:value=rows/>
                            <input type="hidden" name="columns" prop:value=columns/>
                        </ActionForm>

                        <button
                            class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                            on:click=move |_| {
                                set_tableprop(vec![vec![0; rows.get()]; columns.get()]);
                            }
                        >

                            Clear
                        </button>
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

                </Show>
                <Show when=move || solved.get() != vec![""]>
                    <div class="flex flex-col items-center">
                        <p class="text-violet-700 text-2xl my-4">"Solved Puzzle:"</p>
                        <SolvedPuzzle solved=solved/>
                    </div>
                    <button
                        class="w-24 h-12 rounded px-2 bg-violet-700 text-slate-100 cursor-pointer"
                        on:click=move |_| {
                            set_solved(vec!["".to_string()]);
                        }
                    >

                        Back
                    </button>

                </Show>
            </div>
        </main>
    }
}

#[component]
fn SolvedPuzzle(
    solved: ReadSignal<Vec<String>>,
) -> impl IntoView {

    let solved_chars: Vec<Vec<char>> = solved.get().into_iter().map(|s| s.chars().collect()).collect();

    view! {
        <For
            each=move || solved_chars.clone().into_iter()
            key=move |counter| counter.clone()
            children=move |row| {
                view! {
                    <div class="flex flex-row">
                        <For
                            each=move || row.clone().into_iter()
                            key=move |ch| *ch
                            children=move |ch| {
                                view! { <CharComponent ch=ch/> }
                            }
                        />

                    </div>
                }
            }
        />
    }
}

#[component]
fn CharComponent(ch: char) -> impl IntoView {
    match ch {
        '1'..='8' => {view! {
            <div class="h-16 w-16 bg-violet-700 rounded-xl flex items-center justify-center text-slate-100 text-lg">
                {ch.to_string()}
            </div>
        } }
        '.' => {view! { <div class="h-16 w-16 bg-transparent justify-center"></div> }}
        '-' => {view! {
            <div class="h-16 w-16 bg-transparent flex items-center">
                <div class="border border-violet-700 w-full"></div>
            </div>
        }}
        '=' => {view! {
            <div class="h-16 w-16 bg-transparent flex flex-col justify-center space-y-2">
                <div class="border border-violet-700 w-full"></div>
                <div class="border border-violet-700 w-full"></div>
            </div>
        }}
        '|' => {view! {
            <div class="h-16 w-16 bg-transparent flex justify-center">
                <div class="border border-violet-700 h-full"></div>
            </div>
        }}
        '‖' => {view! {
            <div class="h-16 w-16 bg-transparent flex flex-row justify-center space-x-2">
                <div class="border border-violet-700 h-full"></div>
                <div class="border border-violet-700 h-full"></div>
            </div>
        }}
        _ => {view! { <div></div> }},
    }
}

#[component]
fn Table(
    rows: ReadSignal<usize>,
    columns: ReadSignal<usize>,
    tableprop: ReadSignal<Vec<Vec<u8>>>,
    set_tableprop: WriteSignal<Vec<Vec<u8>>>,
) -> impl IntoView {

    view! {
        <div class="flex flex-col items-center">
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

        </div>
    }
}

#[component]
fn Column(
    columns: ReadSignal<usize>,
    tableprop: ReadSignal<Vec<Vec<u8>>>,
    set_tableprop: WriteSignal<Vec<Vec<u8>>>,
    rowid: usize,
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
                            prop:value=move || {
                                tableprop
                                    .with(|vec| {
                                        vec.get(id as usize)
                                            .and_then(|row| row.get(rowid as usize))
                                            .map(|n| n.to_string())
                                    })
                                    .unwrap_or_default()
                            }

                            min="0"
                            max="8"
                            class="h-16 w-16 border bg-violet-700 text-center text-slate-100 text-lg"
                            on:input=move |value| {
                                let mut table = tableprop.get();
                                table[id][rowid] = event_target_value(&value).parse().unwrap_or(0);
                                set_tableprop(table);
                            }
                        />
                    }
                }
            />

        </div>
    }
}

use backend::parse_input::parse_vec_input;
use backend::solver::solve;
use backend::solver::get_content;
use backend::generate_clauses::generate;
use backend::writer::generate_dimacs;
use backend::reconstruct::reconstruct_puzzle;
#[server(Solve, "/api")]
pub async fn solvepuzzle(tableprop: String, rows: usize) -> Result<String, ServerFnError> {
    let oldtable: Vec<Vec<u8>> = serde_json::from_str(&tableprop)?;
    let table = oldtable.into_iter().fold(vec![vec![]; rows], |mut acc, x| {
        for (i, y) in x.into_iter().enumerate() {
            acc[i].push(y);
        }
        acc
    });
    let game_board = parse_vec_input(table);
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

#[server(ToCNF, "/api")]
pub async fn to_cnf(tableprop: String, rows: usize) -> Result<String, ServerFnError> {
    let oldtable: Vec<Vec<u8>> = serde_json::from_str(&tableprop)?;
    let table = oldtable.into_iter().fold(vec![vec![]; rows], |mut acc, x| {
        for (i, y) in x.into_iter().enumerate() {
            acc[i].push(y);
        }
        acc
    });
    let game_board = parse_vec_input(table);
    match game_board {
        Ok(game_board) => {
            let (clauses, var_map) = generate(&game_board);
            let mut output = String::new();

            output.push_str(&format!("c DIMACS file generated\n"));
            output.push_str(&format!("p cnf {} {}\n", var_map.keys().len(), clauses.len()));

            for clause in clauses {
                for literal in clause {
                    output.push_str(&format!("{} ", literal));
                }
                output.push_str("0\n");
            }
            Ok(output)
        }
        Err(e) => Err(ServerFnError::ServerError(e.to_string()))
    }
}

#[server(Generate, "/api")]
pub async fn generatfield(rows: usize, columns: usize) -> Result<Vec<Vec<u8>>, ServerFnError> {
    let grid = generator(rows, columns);
    let newgrid = grid.into_iter().fold(vec![vec![]; columns], |mut acc, x| {
        for (i, y) in x.into_iter().enumerate() {
            acc[i].push(y);
        }
        acc
    });
    Ok(newgrid)
}

