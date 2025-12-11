pub fn root_response() -> wry::http::Response<Vec<u8>> {
    // Serve the main HTML page
    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <title>Wry Test</title>
    <script>{}</script>
</head>
<body>
    <h1 id="click-count">Button not clicked yet</h1>
</body>
</html>"#, include_str!("./js/heap.js"));

    wry::http::Response::builder()
        .header("Content-Type", "text/html")
        .body(html.as_bytes().to_vec())
        .map_err(|e| e.to_string())
        .expect("Failed to build response")
}
