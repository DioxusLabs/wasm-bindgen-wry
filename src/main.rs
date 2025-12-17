//! Example application using wry-testing library

use wry_testing::{
    WINDOW, batch, run, set_on_log, wait_for_js_event
};

fn main() -> wry::Result<()> {
    run(app)
}

fn app() {
    set_on_log(Box::new(|msg: String| {
        println!("Log from JS: {}", msg);
    }));

    batch(|| {
        // Get document body using the lazily-initialized WINDOW static
        let document = WINDOW.with(|window| window.document());
        let body = document.body();

        // Create a container div
        let container = document.create_element("div".to_string());
        container.set_attribute("id".to_string(), "heap-demo".to_string());
        container.set_attribute("style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

        // Create a heading
        let heading = document.create_element("h2".to_string());
        heading.set_text_content("JSHeap Demo".to_string());
        heading.set_attribute(
            "style".to_string(),
            "color: #333; margin-top: 0;".to_string(),
        );
        container.append_child(heading);

        // Create a counter display
        let counter_display = document.create_element("p".to_string());
        counter_display.set_attribute("id".to_string(), "heap-counter".to_string());
        counter_display.set_attribute(
            "style".to_string(),
            "font-size: 24px; font-weight: bold; color: #2196F3;".to_string(),
        );
        counter_display.set_text_content("Counter: 0".to_string());
        container.append_child(counter_display.clone());

        // Create a button
        let button = document.create_element("button".to_string());
        button.set_text_content("Click me (heap-managed)".to_string());
        button.set_attribute("id".to_string(), "heap-button".to_string());
        button.set_attribute("style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
        container.append_child(button);
        // Append container to body
        body.append_child(container);

        let counter_ref = counter_display.clone();
        // Demo 4: Event handling with heap refs
        let mut count = 0;
        body.add_event_listener(
            "click".to_string(),
            Box::new(move || {
                count += 1;

                // Update the counter display using the heap ref
                let start = std::time::Instant::now();
                counter_ref.set_text_content(format!("Counter: {}", count));
                let duration = start.elapsed();
                println!(
                    "Updated counter display in {:?} microseconds",
                    duration.as_micros()
                );
            }),
        );
    });

    // Keep running to handle events
    wait_for_js_event::<()>();
}
