use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub fn main() {
    wry_testing::run(|| async {
        app();
        std::future::pending::<()>().await;
    })
    .unwrap();
}

fn app() {
    yew::Renderer::<App>::new().render();
}

#[function_component(App)]
fn app_component() -> Html {
    let counter = use_state(|| 0);
    let increment_count: Callback<_> = use_callback(counter.clone(), {
        move |_, counter| {
            println!("Incrementing counter to {}", **counter + 1);
            counter.set(**counter + 1);
        }
    });

    // Auto-increment using spawn_local
    use_effect_with(increment_count, move |increment_count| {
        let increment_count = increment_count.clone();
        spawn_local(async move {
            tokio::time::sleep(std::time::Duration::from_millis(0)).await;
            increment_count.emit(());
        });
    });

    html! {
        <>
            <div style="font-family: sans-serif; padding: 20px;">
                <h1>{ "Yew + Wry Counter" }</h1>
                <p style="font-size: 48px;">{ *counter }</p>
            </div>
            <Counter />
        </>
    }
}

#[function_component]
fn Counter() -> Html {
    let state = use_state(|| 0);

    let incr_counter = {
        let state = state.clone();
        Callback::from(move |_| state.set(*state + 1))
    };

    let decr_counter = {
        let state = state.clone();
        Callback::from(move |_| state.set(*state - 1))
    };

    html! {
        <>
            <p> {"current count: "} {*state} </p>
            <button onclick={incr_counter}> {"+"} </button>
            <button onclick={decr_counter}> {"-"} </button>
        </>
    }
}
