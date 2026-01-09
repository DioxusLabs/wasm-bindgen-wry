use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

pub fn main() {
    wry_testing::run(|| async {
        app();
        std::future::pending::<()>().await
    })
    .unwrap();
}

#[wasm_bindgen(inline_js = r#"
export function loadLeaflet() {
    return new Promise((resolve, reject) => {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
        document.head.appendChild(link);

        const script = document.createElement('script');
        script.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
        script.onload = () => resolve();
        script.onerror = () => reject(new Error('Failed to load Leaflet'));
        document.head.appendChild(script);
    });
}

export function createMap(elementId, lat, lng, zoom) {
    const map = L.map(elementId).setView([lat, lng], zoom);
    return map;
}

export function addTileLayer(map, url, attribution, maxZoom) {
    const layer = L.tileLayer(url, { maxZoom, attribution });
    layer.addTo(map);
    return layer;
}

export function addLayerControl(map, baseMaps) {
    L.control.layers(baseMaps).addTo(map);
}

export function createTileLayer(url, attribution, maxZoom) {
    return L.tileLayer(url, { maxZoom, attribution });
}

export function addMarker(map, lat, lng, popupText, draggable) {
    const marker = L.marker([lat, lng], { draggable: draggable || false }).addTo(map);
    if (popupText) {
        marker.bindPopup(popupText);
    }
    return marker;
}

export function openPopup(marker) {
    marker.openPopup();
}

export function removeMarker(map, marker) {
    map.removeLayer(marker);
}

export function setMarkerPopup(marker, content) {
    marker.setPopupContent(content);
}

export function setView(map, lat, lng, zoom) {
    map.setView([lat, lng], zoom);
}

export function onMapClick(map, callback) {
    map.on('click', (e) => callback(e.latlng.lat, e.latlng.lng));
}

export function onMapMouseMove(map, callback) {
    map.on('mousemove', (e) => callback(e.latlng.lat, e.latlng.lng));
}

export function onMarkerDragEnd(marker, callback) {
    marker.on('dragend', () => {
        const ll = marker.getLatLng();
        callback(ll.lat, ll.lng);
    });
}

export function searchNominatim(query) {
    return fetch(`https://nominatim.openstreetmap.org/search?format=json&q=${encodeURIComponent(query)}`)
        .then(r => r.json());
}

export function getCurrentPosition() {
    return new Promise((resolve, reject) => {
        if (navigator.geolocation) {
            navigator.geolocation.getCurrentPosition(
                (pos) => resolve({ lat: pos.coords.latitude, lng: pos.coords.longitude }),
                (err) => {
                    // Fall back to IP-based geolocation
                    fetch('https://ipapi.co/json/')
                        .then(r => r.json())
                        .then(data => resolve({ lat: data.latitude, lng: data.longitude }))
                        .catch(() => reject(err.message));
                },
                { timeout: 5000 }
            );
        } else {
            // Fall back to IP-based geolocation
            fetch('https://ipapi.co/json/')
                .then(r => r.json())
                .then(data => resolve({ lat: data.latitude, lng: data.longitude }))
                .catch(() => reject('Geolocation not supported'));
        }
    });
}
"#)]
extern "C" {
    fn loadLeaflet() -> js_sys::Promise;
    fn createMap(element_id: &str, lat: f64, lng: f64, zoom: u32) -> JsValue;
    fn addTileLayer(map: &JsValue, url: &str, attribution: &str, max_zoom: u32) -> JsValue;
    fn addLayerControl(map: &JsValue, base_maps: &JsValue);
    fn createTileLayer(url: &str, attribution: &str, max_zoom: u32) -> JsValue;
    fn addMarker(map: &JsValue, lat: f64, lng: f64, popup_text: &str, draggable: bool) -> JsValue;
    fn openPopup(marker: &JsValue);
    fn removeMarker(map: &JsValue, marker: &JsValue);
    fn setMarkerPopup(marker: &JsValue, content: &str);
    fn setView(map: &JsValue, lat: f64, lng: f64, zoom: u32);
    fn onMapClick(map: &JsValue, callback: &Closure<dyn Fn(f64, f64)>);
    fn onMapMouseMove(map: &JsValue, callback: &Closure<dyn Fn(f64, f64)>);
    fn onMarkerDragEnd(marker: &JsValue, callback: &Closure<dyn Fn(f64, f64)>);
    fn searchNominatim(query: &str) -> js_sys::Promise;
    fn getCurrentPosition() -> js_sys::Promise;
}

struct AppState {
    map: JsValue,
    markers: Vec<JsValue>,
}

fn app() {
    console_error_panic_hook::set_once();

    let document = web_sys::window().unwrap_throw().document().unwrap_throw();

    // Set up styles
    let style = document.create_element("style").unwrap_throw();
    style.set_text_content(Some(
        r#"
        html, body { height: 100%; margin: 0; padding: 0; font-family: sans-serif; }
        #map { height: 100%; width: 100%; }
        #controls {
            position: absolute;
            top: 10px;
            right: 10px;
            z-index: 1000;
            background: white;
            padding: 10px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.2);
            display: flex;
            flex-direction: column;
            gap: 8px;
            max-width: 250px;
        }
        #controls input[type="text"] {
            padding: 8px;
            border: 1px solid #ccc;
            border-radius: 4px;
            width: 100%;
            box-sizing: border-box;
        }
        #controls button {
            padding: 8px 12px;
            border: none;
            border-radius: 4px;
            background: #0078A8;
            color: white;
            cursor: pointer;
            font-size: 14px;
        }
        #controls button:hover { background: #005f85; }
        #controls button.danger { background: #d33; }
        #controls button.danger:hover { background: #a00; }
        #coords {
            position: absolute;
            bottom: 10px;
            left: 10px;
            z-index: 1000;
            background: rgba(255,255,255,0.9);
            padding: 5px 10px;
            border-radius: 4px;
            font-size: 12px;
            font-family: monospace;
        }
        #marker-count {
            font-size: 12px;
            color: #666;
        }
        "#,
    ));
    document
        .head()
        .unwrap_throw()
        .append_child(&style)
        .unwrap_throw();

    // Create UI
    let body = document.body().unwrap_throw();
    body.set_inner_html(
        r#"
        <div id="map"></div>
        <div id="controls">
            <input type="text" id="search-input" placeholder="Search location..." />
            <button id="search-btn">Search</button>
            <button id="location-btn">My Location</button>
            <button id="clear-btn" class="danger">Clear Markers</button>
            <span id="marker-count">Markers: 0</span>
        </div>
        <div id="coords">Move mouse over map</div>
        "#,
    );

    spawn_local(async {
        // Load Leaflet
        if let Err(e) = wasm_bindgen_futures::JsFuture::from(loadLeaflet()).await {
            web_sys::console::error_1(&e);
            return;
        }

        // Create map
        let map = createMap("map", 37.7749, -122.4194, 13);

        // Add tile layers
        let osm = createTileLayer(
            "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
            "&copy; OpenStreetMap",
            19,
        );
        let satellite = createTileLayer(
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            "&copy; Esri",
            19,
        );
        let topo = createTileLayer(
            "https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png",
            "&copy; OpenTopoMap",
            17,
        );

        // Add default layer
        addTileLayer(
            &map,
            "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
            "&copy; OpenStreetMap",
            19,
        );

        // Create layer control object
        let base_maps = js_sys::Object::new();
        js_sys::Reflect::set(&base_maps, &"Street".into(), &osm).unwrap_throw();
        js_sys::Reflect::set(&base_maps, &"Satellite".into(), &satellite).unwrap_throw();
        js_sys::Reflect::set(&base_maps, &"Topographic".into(), &topo).unwrap_throw();
        addLayerControl(&map, &base_maps);

        // Shared state
        let state = Rc::new(RefCell::new(AppState {
            map: map.clone(),
            markers: Vec::new(),
        }));

        // Add initial marker
        let marker = addMarker(
            &map,
            37.7749,
            -122.4194,
            "San Francisco - Click anywhere to add markers!",
            false,
        );
        openPopup(&marker);
        state.borrow_mut().markers.push(marker);
        update_marker_count(state.borrow().markers.len());

        // Mouse move handler for coordinates
        let mouse_move_cb = Closure::wrap(Box::new(move |lat: f64, lng: f64| {
            let document = web_sys::window().unwrap_throw().document().unwrap_throw();
            if let Some(coords) = document.get_element_by_id("coords") {
                coords.set_text_content(Some(&format!("Lat: {:.5}, Lng: {:.5}", lat, lng)));
            }
        }) as Box<dyn Fn(f64, f64)>);
        onMapMouseMove(&map, &mouse_move_cb);

        // Click handler to add markers
        let state_click = state.clone();
        let map_click = map.clone();
        let click_cb = Closure::wrap(Box::new(move |lat: f64, lng: f64| {
            let marker = addMarker(
                &map_click,
                lat,
                lng,
                &format!("Lat: {:.5}<br>Lng: {:.5}", lat, lng),
                true,
            );
            openPopup(&marker);

            // Drag end handler
            let marker_ref = marker.clone();
            let drag_cb = Closure::wrap(Box::new(move |new_lat: f64, new_lng: f64| {
                setMarkerPopup(
                    &marker_ref,
                    &format!("Lat: {:.5}<br>Lng: {:.5}", new_lat, new_lng),
                );
            }) as Box<dyn Fn(f64, f64)>);
            onMarkerDragEnd(&marker, &drag_cb);

            state_click.borrow_mut().markers.push(marker);
            update_marker_count(state_click.borrow().markers.len());
        }) as Box<dyn Fn(f64, f64)>);
        onMapClick(&map, &click_cb);

        // Search button
        let state_search = state.clone();
        let map_search = map.clone();
        setup_search_handler(state_search, map_search);

        // My Location button
        let state_loc = state.clone();
        let map_loc = map.clone();
        setup_location_handler(state_loc, map_loc);

        // Clear markers button
        let state_clear = state.clone();
        setup_clear_handler(state_clear);
    });
}

fn update_marker_count(count: usize) {
    let document = web_sys::window().unwrap_throw().document().unwrap_throw();
    if let Some(el) = document.get_element_by_id("marker-count") {
        el.set_text_content(Some(&format!("Markers: {}", count)));
    }
}

fn setup_search_handler(state: Rc<RefCell<AppState>>, map: JsValue) {
    let document = web_sys::window().unwrap_throw().document().unwrap_throw();

    let do_search = {
        let state = state.clone();
        let map = map.clone();
        move || {
            let document = web_sys::window().unwrap_throw().document().unwrap_throw();
            let input: web_sys::HtmlInputElement = document
                .get_element_by_id("search-input")
                .unwrap_throw()
                .dyn_into()
                .unwrap_throw();
            let query = input.value();
            if query.is_empty() {
                return;
            }

            let state = state.clone();
            let map = map.clone();
            spawn_local(async move {
                let promise = searchNominatim(&query);
                if let Ok(result) = wasm_bindgen_futures::JsFuture::from(promise).await {
                    let results: js_sys::Array = result.dyn_into().unwrap_throw();
                    if results.length() > 0 {
                        let first = results.get(0);
                        let lat: f64 = js_sys::Reflect::get(&first, &"lat".into())
                            .unwrap_throw()
                            .as_string()
                            .unwrap_throw()
                            .parse()
                            .unwrap_throw();
                        let lon: f64 = js_sys::Reflect::get(&first, &"lon".into())
                            .unwrap_throw()
                            .as_string()
                            .unwrap_throw()
                            .parse()
                            .unwrap_throw();
                        let name = js_sys::Reflect::get(&first, &"display_name".into())
                            .unwrap_throw()
                            .as_string()
                            .unwrap_throw();

                        setView(&map, lat, lon, 14);
                        let marker = addMarker(&map, lat, lon, &name, false);
                        openPopup(&marker);
                        state.borrow_mut().markers.push(marker);
                        update_marker_count(state.borrow().markers.len());
                    }
                }
            });
        }
    };

    // Search button click
    let search_btn = document.get_element_by_id("search-btn").unwrap_throw();
    let do_search_clone = do_search.clone();
    let search_closure = Closure::wrap(Box::new(move || {
        do_search_clone();
    }) as Box<dyn Fn()>);
    search_btn
        .add_event_listener_with_callback("click", search_closure.as_ref().unchecked_ref())
        .unwrap_throw();

    // Enter key in search input
    let input = document.get_element_by_id("search-input").unwrap_throw();
    let enter_closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
        if e.key() == "Enter" {
            do_search();
        }
    }) as Box<dyn Fn(web_sys::KeyboardEvent)>);
    input
        .add_event_listener_with_callback("keypress", enter_closure.as_ref().unchecked_ref())
        .unwrap_throw();
}

fn setup_location_handler(state: Rc<RefCell<AppState>>, map: JsValue) {
    let document = web_sys::window().unwrap_throw().document().unwrap_throw();
    let location_btn = document.get_element_by_id("location-btn").unwrap_throw();

    let location_closure = Closure::wrap(Box::new(move || {
        let state = state.clone();
        let map = map.clone();

        // Show loading state
        let document = web_sys::window().unwrap_throw().document().unwrap_throw();
        if let Some(btn) = document.get_element_by_id("location-btn") {
            btn.set_text_content(Some("Locating..."));
        }

        spawn_local(async move {
            let promise = getCurrentPosition();
            let document = web_sys::window().unwrap_throw().document().unwrap_throw();

            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(result) => {
                    let lat: f64 = js_sys::Reflect::get(&result, &"lat".into())
                        .unwrap_throw()
                        .as_f64()
                        .unwrap_throw();
                    let lng: f64 = js_sys::Reflect::get(&result, &"lng".into())
                        .unwrap_throw()
                        .as_f64()
                        .unwrap_throw();

                    setView(&map, lat, lng, 15);
                    let marker = addMarker(&map, lat, lng, "You are here!", false);
                    openPopup(&marker);
                    state.borrow_mut().markers.push(marker);
                    update_marker_count(state.borrow().markers.len());
                }
                Err(e) => {
                    web_sys::console::error_1(&e);
                }
            }

            // Reset button text
            if let Some(btn) = document.get_element_by_id("location-btn") {
                btn.set_text_content(Some("My Location"));
            }
        });
    }) as Box<dyn Fn()>);
    location_btn
        .add_event_listener_with_callback("click", location_closure.as_ref().unchecked_ref())
        .unwrap_throw();
}

fn setup_clear_handler(state: Rc<RefCell<AppState>>) {
    let document = web_sys::window().unwrap_throw().document().unwrap_throw();
    let clear_btn = document.get_element_by_id("clear-btn").unwrap_throw();

    let clear_closure = Closure::wrap(Box::new(move || {
        let mut state = state.borrow_mut();
        let map = state.map.clone();
        for marker in state.markers.drain(..) {
            removeMarker(&map, &marker);
        }
        update_marker_count(0);
    }) as Box<dyn Fn()>);
    clear_btn
        .add_event_listener_with_callback("click", clear_closure.as_ref().unchecked_ref())
        .unwrap_throw();
}
