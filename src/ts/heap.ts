// Type definitions
type SyncRequestContents = RespondPayload | EvaluatePayload;

interface RespondPayload {
  Respond: {
    response: unknown;
  };
}

interface EvaluatePayload {
  Evaluate: {
    fn_id: number;
    args: SerializedArg[];
  };
}

interface ResponseFromRust {
  Respond?: {
    response: unknown;
  };
  Evaluate?: {
    fn_id: number;
    args: SerializedArg[];
  };
}

type SerializedArg =
  | string
  | number
  | SerializedArg[]
  | SerializedFunction
  | { [key: string]: SerializedArg };

interface SerializedFunction {
  type: "function";
  id: number;
}

// This function sends the event to the virtualdom and then waits for the virtualdom to process it
//
// However, it's not really suitable for liveview, because it's synchronous and will block the main thread
// We should definitely consider using a websocket if we want to block... or just not block on liveview
// Liveview is a little bit of a tricky beast
function sync_request(endpoint: string, contents: SyncRequestContents): ResponseFromRust | null {
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
  const contents_base64 = btoa(String.fromCharCode.apply(null, contents_bytes as unknown as number[]));
  xhr.setRequestHeader("dioxus-data", contents_base64);
  xhr.send();

  const response_text = xhr.responseText;
  console.log("Received response from Rust:", response_text);
  try {
    return JSON.parse(response_text) as ResponseFromRust;
  } catch (e) {
    console.error("Failed to parse response JSON:", e);
    return null;
  }
}

function run_code(code: number, args: unknown[]): unknown {
  let f: (...args: unknown[]) => unknown;
  switch (code) {
    case 0:
      f = console.log;
      break;
    case 1:
      f = alert;
      break;
    case 2:
      f = function (a: unknown, b: unknown) {
        return a + b;
      };
      break;
    case 3:
      f = function (event_name: string, callback: RustFunction): void {
        document.addEventListener(event_name, function (e: Event): void {
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
      f = function (element_id: string, text_content: string): void {
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

function evaluate_from_rust(code: number, args_json: SerializedArg[]): unknown {
  let args = deserialize_args(args_json) as unknown[];
  const result = run_code(code, args);
  const response: RespondPayload = {
    Respond: {
      response: result || null,
    },
  };
  const request_result = sync_request("wry://handler", response);
  return handleResponse(request_result);
}

function deserialize_args(args_json: SerializedArg): unknown {
  if (typeof args_json === "string") {
    return args_json;
  } else if (typeof args_json === "number") {
    return args_json;
  } else if (Array.isArray(args_json)) {
    return args_json.map(deserialize_args);
  } else if (typeof args_json === "object" && args_json !== null) {
    if ((args_json as SerializedFunction).type === "function") {
      return new RustFunction((args_json as SerializedFunction).id);
    } else {
      const obj: { [key: string]: unknown } = {};
      for (const key in args_json) {
        obj[key] = deserialize_args((args_json as { [key: string]: SerializedArg })[key]);
      }
      return obj;
    }
  }
}

function handleResponse(response: ResponseFromRust | null): unknown {
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
  code: number;

  constructor(code: number) {
    this.code = code;
  }

  call(...args: unknown[]): unknown {
    const response = sync_request("wry://handler", {
      Evaluate: {
        fn_id: this.code,
        args: args as SerializedArg[],
      },
    });
    return handleResponse(response);
  }
}

// @ts-ignore
window.evaluate_from_rust = evaluate_from_rust;