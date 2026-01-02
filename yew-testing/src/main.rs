use yew::prelude::*;
use yew_hooks::prelude::*;

pub fn main() {
    wry_testing::run(|| async {
        app();
        std::future::pending::<()>().await;
    })
    .unwrap();
}

fn app() {
    yew::Renderer::<Async>::new().render();
}

#[function_component(Async)]
fn async_test() -> Html {
    let state = use_async(async move { fetch("https://dioxuslabs.com/".to_string()).await });

    use_effect({
        let state = state.clone();
        move || {
            state.run();
        }
    });

    html! {
        <div>
            {
                if state.loading {
                    html! { "Loading" }
                } else {
                    html! {}
                }
            }
            {
                if let Some(data) = &state.data {
                    html! { data }
                } else {
                    html! {}
                }
            }
            {
                if let Some(error) = &state.error {
                    html! { error }
                } else {
                    html! {}
                }
            }
        </div>
    }
}

async fn fetch(url: String) -> Result<String, String> {
    reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}
