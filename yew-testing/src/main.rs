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
            println!("Incrementing counter");
            counter.set(**counter + 1);
        }
    });

    // Auto-increment using spawn_local
    use_effect_with(increment_count, move |increment_count| {
        let increment_count = increment_count.clone();
        spawn_local(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                increment_count.emit(());
            }
        });
    });

    html! {
        <div style="font-family: sans-serif; padding: 20px;">
            <h1>{ "Yew + Wry Counter" }</h1>
            <p style="font-size: 48px;">{ *counter }</p>
        </div>
    }
}
