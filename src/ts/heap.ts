// This function sends the event to the virtualdom and then waits for the virtualdom to process it
//
// However, it's not really suitable for liveview, because it's synchronous and will block the main thread
// We should definitely consider using a websocket if we want to block... or just not block on liveview
// Liveview is a little bit of a tricky beast
function sync_request(endpoint, contents) {
  // Handle the event on the virtualdom and then process whatever its output was
  const xhr = new XMLHttpRequest();

  // Serialize the event and send it to the custom protocol in the Rust side of things
  xhr.open("POST", endpoint, false);
  xhr.setRequestHeader("Content-Type", "application/json");

  // hack for android since we CANT SEND BODIES (because wry is using shouldInterceptRequest)
  //
  // https://issuetracker.google.com/issues/119844519
  // https://stackoverflow.com/questions/43273640/android-webviewclient-how-to-get-post-request-body
  // https://developer.android.com/reference/android/webkit/WebViewClient#shouldInterceptRequest(android.webkit.WebView,%20android.webkit.WebResourceRequest)
  //
  // the issue here isn't that big, tbh, but there's a small chance we lose the event due to header max size (16k per header, 32k max)
  const json_string = JSON.stringify(contents);
  console.log("Sending request to Rust:", json_string);
  const contents_bytes = new TextEncoder().encode(json_string);
  const contents_base64 = btoa(String.fromCharCode.apply(null, contents_bytes));
  xhr.setRequestHeader("dioxus-data", contents_base64);
  xhr.send();

  const response_text = xhr.responseText;
  console.log("Received response from Rust:", response_text);
  try {
    return JSON.parse(response_text);
  } catch (e) {
    console.error("Failed to parse response JSON:", e);
    return null;
  }
}

function run_code(code, args) {
  let f;
  switch (code) {
    case 0:
      f = console.log;
      break;
    case 1:
      f = alert;
      break;
    case 2:
      f = function (a, b) {
        return a + b;
      };
      break;
    case 3:
      f = function (event_name, callback) {
        document.addEventListener(event_name, function (e) {
          if (callback.call()) {
            e.preventDefault();
            console.log(
              "Event " + event_name + " default prevented by Rust callback."
            );
          }
        });
      };
      break;
    case 4:
      f = function (element_id, text_content) {
        const element = document.getElementById(element_id);
        if (element) {
          element.textContent = text_content;
        } else {
          console.warn("Element with ID " + element_id + " not found.");
        }
      };
      break;
    default:
      throw new Error("Unknown code: " + code);
  }
  return f.apply(null, args);
}

function evaluate_from_rust(code, args_json) {
  let args = deserialize_args(args_json);
  const result = run_code(code, args);
  const response = {
    Respond: {
      response: result || null,
    },
  };
  const request_result = sync_request("wry://handler", response);
  return handleResponse(request_result);
}

function deserialize_args(args_json) {
  if (typeof args_json === "string") {
    return args_json;
  } else if (typeof args_json === "number") {
    return args_json;
  } else if (Array.isArray(args_json)) {
    return args_json.map(deserialize_args);
  } else if (typeof args_json === "object" && args_json !== null) {
    if (args_json.type === "function") {
      return new RustFunction(args_json.id);
    } else {
      const obj = {};
      for (const key in args_json) {
        obj[key] = deserialize_args(args_json[key]);
      }
      return obj;
    }
  }
}

function handleResponse(response) {
  if (!response) {
    return;
  }
  console.log("Handling response:", response);
  if (response.Respond) {
    return response.Respond.response;
  } else if (response.Evaluate) {
    return evaluate_from_rust(response.Evaluate.fn_id, response.Evaluate.args);
  } else {
    throw new Error("Unknown response type");
  }
}

class RustFunction {
  constructor(code) {
    this.code = code;
  }

  call(...args) {
    const response = sync_request("wry://handler", {
      Evaluate: {
        fn_id: this.code,
        args: args,
      },
    });
    return handleResponse(response);
  }
}
