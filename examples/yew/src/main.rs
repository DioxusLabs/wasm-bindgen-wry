// https://github.com/yewstack/yew/blob/master/examples/todomvc adapted for wry-testing
use gloo::storage::{LocalStorage, Storage};
use gloo::utils::window;
use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use web_sys::HtmlInputElement as InputElement;
use yew::events::{FocusEvent, KeyboardEvent};
use yew::html::IntoPropValue;
use yew::html::Scope;
use yew::{Classes, Component, Context, Html, NodeRef, TargetCast, classes, html};

pub fn main() {
    wry_testing::run(|| async {
        app();
        std::future::pending::<()>().await;
    })
    .unwrap();
}

fn app() {
    window().document().unwrap().head().unwrap().set_inner_html(
        r#"<meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Yew • TodoMVC</title>
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
    yew::Renderer::<App>::new().render();
}

const KEY: &str = "yew.todomvc.self";

pub enum Msg {
    Add(String),
    Edit((usize, String)),
    Remove(usize),
    SetFilter(Filter),
    ToggleAll,
    ToggleEdit(usize),
    Toggle(usize),
    ClearCompleted,
    Focus,
}

pub struct App {
    state: State,
    focus_ref: NodeRef,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let entries = LocalStorage::get(KEY).unwrap_or_else(|_| Vec::new());
        let state = State::new(entries);
        let focus_ref = NodeRef::default();
        Self { state, focus_ref }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Add(description) => {
                let description = description.trim();
                if !description.is_empty() {
                    let entry = Entry {
                        description: description.to_string(),
                        completed: false,
                        editing: false,
                    };
                    self.state.entries.push(entry);
                }
            }
            Msg::Edit((idx, edit_value)) => {
                self.state.complete_edit(idx, edit_value.trim().to_string());
                self.state.edit_value = "".to_string();
            }
            Msg::Remove(idx) => {
                self.state.remove(idx);
            }
            Msg::SetFilter(filter) => {
                self.state.filter = filter;
            }
            Msg::ToggleEdit(idx) => {
                let entry = self
                    .state
                    .entries
                    .iter()
                    .filter(|e| self.state.filter.fits(e))
                    .nth(idx)
                    .unwrap();
                self.state.edit_value.clone_from(&entry.description);
                self.state.clear_all_edit();
                self.state.toggle_edit(idx);
            }
            Msg::ToggleAll => {
                let status = !self.state.is_all_completed();
                self.state.set_completed(status);
            }
            Msg::Toggle(idx) => {
                self.state.toggle_completed(idx);
            }
            Msg::ClearCompleted => {
                self.state.clear_completed();
            }
            Msg::Focus => {
                if let Some(input) = self.focus_ref.cast::<InputElement>() {
                    input.focus().unwrap();
                }
            }
        }
        LocalStorage::set(KEY, &self.state.entries).expect("failed to set");
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let hidden_class = if self.state.entries.is_empty() {
            "hidden"
        } else {
            ""
        };
        html! {
            <div class="todomvc-wrapper">
                <section class="todoapp">
                    <header class="header">
                        <h1>{ "todos" }</h1>
                        { self.view_input(ctx.link()) }
                    </header>
                    <section class={classes!("main", hidden_class)}>
                        <input
                            type="checkbox"
                            class="toggle-all"
                            id="toggle-all"
                            checked={self.state.is_all_completed()}
                            onclick={ctx.link().callback(|_| Msg::ToggleAll)}
                        />
                        <label for="toggle-all" />
                        <ul class="todo-list">
                            { for self
                                .state
                                .entries
                                .iter()
                                .enumerate()
                                .filter(|(_, entry)| self.state.filter.fits(entry))
                                .map(|(i, e)| self.view_entry((i, e), ctx.link()))
                            }
                        </ul>
                    </section>
                    <footer class={classes!("footer", hidden_class)}>
                        <span class="todo-count">
                            <strong>{ self.state.total() }</strong>
                            { " item(s) left" }
                        </span>
                        <ul class="filters">
                            { for Filter::iter().map(|flt| self.view_filter(flt, ctx.link())) }
                        </ul>
                        <button class="clear-completed" onclick={ctx.link().callback(|_| Msg::ClearCompleted)}>
                            { format!("Clear completed ({})", self.state.total_completed()) }
                        </button>
                    </footer>
                </section>
                <footer class="info">
                    <p>{ "Double-click to edit a todo" }</p>
                    <p>{ "Written by " }<a href="https://github.com/DenisKolodin/" target="_blank">{ "Denis Kolodin" }</a></p>
                    <p>{ "Part of " }<a href="http://todomvc.com/" target="_blank">{ "TodoMVC" }</a></p>
                </footer>
            </div>
        }
    }
}

impl App {
    fn view_filter(&self, filter: Filter, link: &Scope<Self>) -> Html {
        let cls = if self.state.filter == filter {
            "selected"
        } else {
            "not-selected"
        };
        html! {
            <li>
                <a class={cls}
                   href={filter.as_href()}
                   onclick={link.callback(move |_| Msg::SetFilter(filter))}
                >
                    { filter }
                </a>
            </li>
        }
    }

    fn view_input(&self, link: &Scope<Self>) -> Html {
        let onkeypress = link.batch_callback(|e: KeyboardEvent| {
            if e.key() == "Enter" {
                let input: InputElement = e.target_unchecked_into();
                let value = input.value();
                input.set_value("");
                Some(Msg::Add(value))
            } else {
                None
            }
        });
        html! {
            // You can use standard Rust comments. One line:
            // <li></li>
            <input
                class="new-todo"
                placeholder="What needs to be done?"
                {onkeypress}
            />
            /* Or multiline:
            <ul>
                <li></li>
            </ul>
            */
        }
    }

    fn view_entry(&self, (idx, entry): (usize, &Entry), link: &Scope<Self>) -> Html {
        let mut class = Classes::from("todo");
        if entry.editing {
            class.push(" editing");
        }
        if entry.completed {
            class.push(" completed");
        }
        html! {
            <li {class}>
                <div class="view">
                    <input
                        type="checkbox"
                        class="toggle"
                        checked={entry.completed}
                        onclick={link.callback(move |_| Msg::Toggle(idx))}
                    />
                    <label ondblclick={link.callback(move |_| Msg::ToggleEdit(idx))}>{ &entry.description }</label>
                    <button class="destroy" onclick={link.callback(move |_| Msg::Remove(idx))} />
                </div>
                { self.view_entry_edit_input((idx, entry), link) }
            </li>
        }
    }

    fn view_entry_edit_input(&self, (idx, entry): (usize, &Entry), link: &Scope<Self>) -> Html {
        let edit = move |input: InputElement| {
            let value = input.value();
            input.set_value("");
            Msg::Edit((idx, value))
        };

        let onblur = link.callback(move |e: FocusEvent| edit(e.target_unchecked_into()));

        let onkeypress = link.batch_callback(move |e: KeyboardEvent| {
            (e.key() == "Enter").then(|| edit(e.target_unchecked_into()))
        });

        if entry.editing {
            html! {
                <input
                    class="edit"
                    type="text"
                    ref={self.focus_ref.clone()}
                    value={self.state.edit_value.clone()}
                    onmouseover={link.callback(|_| Msg::Focus)}
                    {onblur}
                    {onkeypress}
                />
            }
        } else {
            html! { <input type="hidden" /> }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub entries: Vec<Entry>,
    pub filter: Filter,
    pub edit_value: String,
}

impl State {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            filter: Filter::All,
            edit_value: "".into(),
        }
    }

    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn total_completed(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| Filter::Completed.fits(e))
            .count()
    }

    pub fn is_all_completed(&self) -> bool {
        let mut filtered_iter = self
            .entries
            .iter()
            .filter(|e| self.filter.fits(e))
            .peekable();

        if filtered_iter.peek().is_none() {
            return false;
        }

        filtered_iter.all(|e| e.completed)
    }

    pub fn clear_completed(&mut self) {
        self.entries.retain(|e| Filter::Active.fits(e));
    }

    pub fn toggle_completed(&mut self, idx: usize) {
        let entry = self.entries.get_mut(idx).unwrap();
        entry.completed = !entry.completed;
    }

    pub fn set_completed(&mut self, value: bool) {
        for entry in &mut self.entries {
            if self.filter.fits(entry) {
                entry.completed = value;
            }
        }
    }

    pub fn toggle_edit(&mut self, idx: usize) {
        let entry = self.entries.get_mut(idx).unwrap();
        entry.editing = !entry.editing;
    }

    pub fn clear_all_edit(&mut self) {
        for entry in &mut self.entries {
            entry.editing = false;
        }
    }

    pub fn complete_edit(&mut self, idx: usize, val: String) {
        if val.is_empty() {
            self.remove(idx);
        } else {
            let entry = self.entries.get_mut(idx).unwrap();
            entry.description = val;
            entry.editing = !entry.editing;
        }
    }

    pub fn remove(&mut self, idx: usize) {
        self.entries.remove(idx);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub description: String,
    pub completed: bool,
    pub editing: bool,
}

#[derive(Clone, Copy, Debug, EnumIter, Display, PartialEq, Serialize, Deserialize, Eq)]
pub enum Filter {
    All,
    Active,
    Completed,
}

impl Filter {
    pub fn fits(&self, entry: &Entry) -> bool {
        match *self {
            Filter::All => true,
            Filter::Active => !entry.completed,
            Filter::Completed => entry.completed,
        }
    }

    pub fn as_href(&self) -> &'static str {
        match self {
            Filter::All => "#/",
            Filter::Active => "#/active",
            Filter::Completed => "#/completed",
        }
    }
}

impl IntoPropValue<Html> for Filter {
    fn into_prop_value(self) -> yew::Html {
        html! { <>{self.to_string()}</> }
    }
}
