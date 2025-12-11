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

class EncodedData {
  private u8Buf: Uint8Array;
  private u8Offset: number;

  private u16Buf: Uint16Array;
  private u16Offset: number;

  private u32Buf: Uint32Array;
  private u32Offset: number;

  private string: string;
  private stringOffset: number;

  constructor(data: ArrayBuffer) {
    const u32Buf = new Uint32Array(data);

    const u32Offset = 3 * 4; // For the three u32 offsets at the start
    this.u32Buf = new Uint32Array(data, 0, u32Offset);
    this.u32Offset = 0;

    const u16Offset = u32Buf[0];
    this.u16Buf = new Uint16Array(data, u16Offset);
    this.u16Offset = 0;

    const u8Offset = u32Buf[1];
    this.u8Buf = new Uint8Array(data, u8Offset);
    this.u8Offset = 0;

    const strOffset = u32Buf[2];
    const strBuf = new Uint8Array(data, strOffset);
    this.string = new TextDecoder("utf-8").decode(strBuf);

    this.stringOffset = 0;
  }

  readU32(): number {
    return this.u32Buf[this.u32Offset++];
  }

  readU16(): number {
    return this.u16Buf[this.u16Offset++];
  }

  readU8(): number {
    return this.u8Buf[this.u8Offset++];
  }

  readString(len: number): string {
    if (this.string === null) {
      throw new Error("No string data available");
    }
    return this.string.substring(this.stringOffset, (this.stringOffset += len));
  }
}

interface SerializedJSHeapRef {
  type: "js_heap_ref";
  id: number;
}

interface SerializedFunction {
  type: "function";
  id: number;
}

type SerializedArg =
  | string
  | number
  | SerializedArg[]
  | SerializedFunction
  | SerializedJSHeapRef
  | { [key: string]: SerializedArg };

// SlotMap implementation for JS heap types
// Uses a free list approach with max ID tracking for efficient slot reuse
class JSHeap {
  private slots: (unknown | undefined)[];
  private freeIds: number[];
  private maxId: number;

  constructor() {
    this.slots = [];
    this.freeIds = [];
    this.maxId = 0;
  }

  // Insert a value and return its unique ID
  insert(value: unknown): number {
    let id: number;
    if (this.freeIds.length > 0) {
      // Reuse a freed slot
      id = this.freeIds.pop()!;
    } else {
      // Allocate a new slot
      id = this.maxId;
      this.maxId++;
    }
    this.slots[id] = value;
    return id;
  }

  // Get a value by ID, returns undefined if not found
  get(id: number): unknown | undefined {
    return this.slots[id];
  }

  // Remove a value by ID and add the slot to the free list
  remove(id: number): unknown | undefined {
    const value = this.slots[id];
    if (value !== undefined) {
      this.slots[id] = undefined;
      this.freeIds.push(id);
    }
    return value;
  }

  // Check if an ID is currently in use
  has(id: number): boolean {
    return this.slots[id] !== undefined;
  }

  // Serialize a value to a JSHeapRef, inserting it into the heap
  serialize(value: unknown): SerializedJSHeapRef {
    const id = this.insert(value);
    return {
      type: "js_heap_ref",
      id: id,
    };
  }
}

// Global JS heap instance for storing arbitrary JS values
const jsHeap = new JSHeap();

// This function sends the event to the virtualdom and then waits for the virtualdom to process it
//
// However, it's not really suitable for liveview, because it's synchronous and will block the main thread
// We should definitely consider using a websocket if we want to block... or just not block on liveview
// Liveview is a little bit of a tricky beast
function sync_request(
  endpoint: string,
  contents: SyncRequestContents
): ResponseFromRust | null {
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
  const contents_base64 = btoa(
    String.fromCharCode.apply(null, contents_bytes as unknown as number[])
  );
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

type AnyFunction = (...args: any[]) => any;

function run_code(code: number, args: unknown[]): unknown {
  let f: AnyFunction;
  switch (code) {
    case 0:
      f = console.log;
      break;
    case 1:
      f = alert;
      break;
    case 2:
      f = (a: number, b: number) => a + b;
      break;
    case 3:
      f = (event_name: string, callback: RustFunction) => {
        document.addEventListener(event_name, (e: Event) => {
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
      f = (element_id: string, text_content: string) => {
        const element = document.getElementById(element_id);
        if (element) {
          element.textContent = text_content;
        } else {
          console.warn("Element with ID " + element_id + " not found.");
        }
      };
      break;
    // JSHeap operations
    case 5:
      // heap_insert: Insert a value into the heap and return a heap ref
      // We explicitly return a SerializedJSHeapRef since serialize_return would
      // pass through plain objects as-is
      f = (value: unknown): SerializedJSHeapRef => {
        const id = jsHeap.insert(value);
        return { type: "js_heap_ref", id: id };
      };
      break;
    case 6:
      // heap_get: Get a value from the heap by its reference (already deserialized)
      f = (value: unknown) => value;
      break;
    case 7:
      // heap_remove: Remove a value from the heap and return it
      f = (id: number) => jsHeap.remove(id);
      break;
    case 8:
      // heap_has: Check if an ID exists in the heap
      f = (id: number) => jsHeap.has(id);
      break;
    case 9:
      // Create a JS object and return a heap reference to it
      // serialize_return handles plain objects, so we need explicit heap insertion
      f = (props: Record<string, unknown>) => {
        const id = jsHeap.insert(props);
        return { type: "js_heap_ref", id: id };
      };
      break;
    case 10:
      // Get a property from a heap object
      f = (obj: Record<string, unknown>, key: string) => obj[key];
      break;
    case 11:
      // Set a property on a heap object
      f = (obj: Record<string, unknown>, key: string, value: unknown) => {
        obj[key] = value;
      };
      break;
    case 12:
      // Call a method on a heap object
      f = (
        obj: Record<string, unknown>,
        method: string,
        methodArgs: unknown[]
      ) => {
        const fn = obj[method];
        if (typeof fn === "function") {
          return fn.apply(obj, methodArgs);
        }
        throw new Error("Method " + method + " is not a function");
      };
      break;
    case 13:
      // Get document.body - serialize_return will auto-convert to heap ref
      f = () => document.body;
      break;
    case 14:
      // querySelector - serialize_return will auto-convert to heap ref
      f = (selector: string) => document.querySelector(selector);
      break;
    case 15:
      // createElement - serialize_return will auto-convert to heap ref
      f = (tag: string) => document.createElement(tag);
      break;
    case 16:
      // appendChild - don't return the child, Rust expects unit
      f = (parent: Element, child: Element) => {
        parent.appendChild(child);
      };
      break;
    case 17:
      // Set element attribute
      f = (el: Element, name: string, value: string) =>
        el.setAttribute(name, value);
      break;
    case 18:
      // Set element textContent
      f = (el: Element, text: string) => {
        el.textContent = text;
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
  const serialized_result = serialize_return(result);
  const response: RespondPayload = {
    Respond: {
      response: serialized_result,
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
    } else if ((args_json as SerializedJSHeapRef).type === "js_heap_ref") {
      // Retrieve the JS object from the heap using its ID
      const id = (args_json as SerializedJSHeapRef).id;
      const value = jsHeap.get(id);
      if (value === undefined) {
        console.warn("JSHeap reference with ID " + id + " not found.");
      }
      return value;
    } else {
      const obj: { [key: string]: unknown } = {};
      for (const key in args_json) {
        obj[key] = deserialize_args(
          (args_json as { [key: string]: SerializedArg })[key]
        );
      }
      return obj;
    }
  }
}

// Serialize a return value for sending back to Rust
// - null/undefined become null
// - Primitives (string, number, boolean) pass through as-is
// - Arrays are recursively serialized
// - Objects that are already SerializedJSHeapRef pass through
// - Plain objects have properties recursively serialized
// - Other objects (DOM elements, etc.) are stored in the heap and returned as refs
function serialize_return(value: unknown): SerializedArg | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return value as SerializedArg;
  }
  if (Array.isArray(value)) {
    return value.map(serialize_return) as SerializedArg[];
  }
  if (typeof value === "object") {
    // Check if it's already a serialized heap ref (has type: "js_heap_ref")
    const obj = value as Record<string, unknown>;
    if (obj.type === "js_heap_ref" && typeof obj.id === "number") {
      return value as SerializedJSHeapRef;
    }
    // Check if it's a plain object (not a DOM node, etc.)
    if (Object.getPrototypeOf(value) === Object.prototype) {
      // Plain object - serialize each property
      const result: { [key: string]: SerializedArg } = {};
      for (const key in obj) {
        result[key] = serialize_return(obj[key]) as SerializedArg;
      }
      return result;
    }
    // Non-plain object (DOM element, etc.) - store in heap
    return jsHeap.serialize(value);
  }
  // Fallback for other types (functions, symbols, etc.) - return null
  return null;
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
// @ts-ignore
window.jsHeap = jsHeap;
