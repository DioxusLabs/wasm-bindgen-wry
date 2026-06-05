// https://github.com/leptos-rs/leptos/tree/main/examples/todomvc adapted for wry-launch
use leptos::{ev, html::Input, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

pub fn main() {
    wry_launch::launch();
}

#[wasm_bindgen(start)]
fn app() {
    console_error_panic_hook::set_once();
    window().document().unwrap().head().unwrap().set_inner_html(
        r#"<meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Leptos • TodoMVC</title>
    <link
      rel="stylesheet"
      href="https://cdn.jsdelivr.net/npm/todomvc-common@1.0.5/base.css"
    />
    <link
      rel="stylesheet"
      href="https://cdn.jsdelivr.net/npm/todomvc-app-css@2.3.0/index.css"
    />

    <link data-trunk rel="rust" />"#,
    );
    leptos::mount::mount_to_body(TodoMVC);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todos(pub Vec<Todo>);

const STORAGE_KEY: &str = "todos-leptos";

impl Default for Todos {
    fn default() -> Self {
        let starting_todos = window()
            .local_storage()
            .ok()
            .flatten()
            .and_then(|storage| {
                storage
                    .get_item(STORAGE_KEY)
                    .ok()
                    .flatten()
                    .and_then(|value| serde_json::from_str::<Vec<Todo>>(&value).ok())
            })
            .unwrap_or_default();
        Self(starting_todos)
    }
}

impl Todos {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn add(&mut self, todo: Todo) {
        self.0.push(todo);
    }

    pub fn remove(&mut self, id: Uuid) {
        self.retain(|todo| todo.id != id);
    }

    pub fn remaining(&self) -> usize {
        self.0.iter().filter(|todo| !todo.completed.get()).count()
    }

    pub fn completed(&self) -> usize {
        self.0.iter().filter(|todo| todo.completed.get()).count()
    }

    pub fn toggle_all(&self) {
        if self.remaining() == 0 {
            for todo in &self.0 {
                todo.completed.update(|completed| {
                    if *completed {
                        *completed = false;
                    }
                });
            }
        } else {
            for todo in &self.0 {
                todo.completed.set(true);
            }
        }
    }

    fn clear_completed(&mut self) {
        self.retain(|todo| !todo.completed.get());
    }

    fn retain(&mut self, mut f: impl FnMut(&Todo) -> bool) {
        self.0.retain(|todo| {
            let retain = f(todo);
            if !retain {
                todo.title.dispose();
                todo.completed.dispose();
            }
            retain
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: Uuid,
    pub title: RwSignal<String>,
    pub completed: RwSignal<bool>,
}

impl Todo {
    pub fn new(id: Uuid, title: String) -> Self {
        Self::new_with_completed(id, title, false)
    }

    pub fn new_with_completed(id: Uuid, title: String, completed: bool) -> Self {
        let title = RwSignal::new(title);
        let completed = RwSignal::new(completed);
        Self {
            id,
            title,
            completed,
        }
    }

    pub fn toggle(&self) {
        self.completed.update(|completed| *completed = !*completed);
    }
}

const ESCAPE_KEY: u32 = 27;
const ENTER_KEY: u32 = 13;

#[component]
pub fn TodoMVC() -> impl IntoView {
    let (todos, set_todos) = signal(Todos::default());
    provide_context(set_todos);

    let (mode, set_mode) = signal(Mode::All);

    window_event_listener(ev::hashchange, move |_| {
        let new_mode = location_hash().map(|hash| route(&hash)).unwrap_or_default();
        set_mode.set(new_mode);
    });

    let input_ref = NodeRef::<Input>::new();
    let add_todo = move |ev: KeyboardEvent| {
        let input = input_ref.get().unwrap();
        ev.stop_propagation();
        if ev.key_code() == ENTER_KEY {
            let title = input.value();
            let title = title.trim();
            if !title.is_empty() {
                let new = Todo::new(Uuid::new_v4(), title.to_string());
                set_todos.update(|t| t.add(new));
                input.set_value("");
            }
        }
    };

    let filtered_todos = move || {
        todos.with(|todos| match mode.get() {
            Mode::All => todos.0.to_vec(),
            Mode::Active => todos
                .0
                .iter()
                .filter(|todo| !todo.completed.get())
                .cloned()
                .collect(),
            Mode::Completed => todos
                .0
                .iter()
                .filter(|todo| todo.completed.get())
                .cloned()
                .collect(),
        })
    };

    Effect::new(move |_| {
        if let Ok(Some(storage)) = window().local_storage() {
            let json = serde_json::to_string(&todos).expect("couldn't serialize Todos");
            if storage.set_item(STORAGE_KEY, &json).is_err() {
                leptos::logging::error!("error while trying to set item in localStorage");
            }
        }
    });

    Effect::new(move |_| {
        if let Some(input) = input_ref.get() {
            let _ = input.focus();
        }
    });

    view! {
        <main>
            <section class="todoapp">
                <header class="header">
                    <h1>"todos"</h1>
                    <input
                        class="new-todo"
                        placeholder="What needs to be done?"
                        autofocus
                        on:keydown=add_todo
                        node_ref=input_ref
                    />
                </header>
                <section class="main" class:hidden=move || todos.with(|t| t.is_empty())>
                    <input
                        id="toggle-all"
                        class="toggle-all"
                        type="checkbox"
                        prop:checked=move || todos.with(|t| t.remaining() > 0)
                        on:input=move |_| todos.with(|t| t.toggle_all())
                    />
                    <label for="toggle-all">"Mark all as complete"</label>
                    <ul class="todo-list">
                        <For each=filtered_todos key=|todo| todo.id let:todo>
                            <Todo todo/>
                        </For>
                    </ul>
                </section>
                <footer class="footer" class:hidden=move || todos.with(|t| t.is_empty())>
                    <span class="todo-count">
                        <strong>{move || todos.with(|t| t.remaining().to_string())}</strong>
                        {move || {
                            if todos.with(|t| t.remaining()) == 1 { " item" } else { " items" }
                        }}
                        " left"
                    </span>
                    <ul class="filters">
                        <li>
                            <a
                                href="#/"
                                class="selected"
                                class:selected=move || mode.get() == Mode::All
                            >
                                "All"
                            </a>
                        </li>
                        <li>
                            <a href="#/active" class:selected=move || mode.get() == Mode::Active>
                                "Active"
                            </a>
                        </li>
                        <li>
                            <a
                                href="#/completed"
                                class:selected=move || mode.get() == Mode::Completed
                            >
                                "Completed"
                            </a>
                        </li>
                    </ul>
                    <button
                        class="clear-completed hidden"
                        class:hidden=move || todos.with(|t| t.completed() == 0)
                        on:click=move |_| set_todos.update(|t| t.clear_completed())
                    >
                        "Clear completed"
                    </button>
                </footer>
            </section>
            <footer class="info">
                <p>"Double-click to edit a todo"</p>
                <p>"Created by " <a href="http://todomvc.com">"Greg Johnston"</a></p>
                <p>"Part of " <a href="http://todomvc.com">"TodoMVC"</a></p>
            </footer>
        </main>
    }
}

#[component]
pub fn Todo(todo: Todo) -> impl IntoView {
    let (editing, set_editing) = signal(false);
    let set_todos = use_context::<WriteSignal<Todos>>().unwrap();
    let todo_input = NodeRef::<Input>::new();

    let save = move |value: &str| {
        let value = value.trim();
        if value.is_empty() {
            set_todos.update(|t| t.remove(todo.id));
        } else {
            todo.title.set(value.to_string());
        }
        set_editing.set(false);
    };

    view! {
        <li class="todo" class:editing=editing class:completed=move || todo.completed.get()>
            <div class="view">
                <input
                    node_ref=todo_input
                    class="toggle"
                    type="checkbox"
                    bind:checked=todo.completed
                />

                <label on:dblclick=move |_| {
                    set_editing.set(true);
                    if let Some(input) = todo_input.get() {
                        _ = input.focus();
                    }
                }>{move || todo.title.get()}</label>
                <button
                    class="destroy"
                    on:click=move |_| set_todos.update(|t| t.remove(todo.id))
                ></button>
            </div>
            {move || {
                editing
                    .get()
                    .then(|| {
                        view! {
                            <input
                                class="edit"
                                class:hidden=move || !editing.get()
                                prop:value=move || todo.title.get()
                                on:focusout:target=move |ev| save(&ev.target().value())
                                on:keyup:target=move |ev| {
                                    let key_code = ev.key_code();
                                    if key_code == ENTER_KEY {
                                        save(&ev.target().value());
                                    } else if key_code == ESCAPE_KEY {
                                        set_editing.set(false);
                                    }
                                }
                            />
                        }
                    })
            }}
        </li>
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Active,
    Completed,
    #[default]
    All,
}

pub fn route(hash: &str) -> Mode {
    match hash {
        "/active" => Mode::Active,
        "/completed" => Mode::Completed,
        _ => Mode::All,
    }
}
