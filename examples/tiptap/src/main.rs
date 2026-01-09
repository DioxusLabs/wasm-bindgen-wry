//! TipTap WYSIWYG Editor Example
//!
//! This example demonstrates how to create a rich text editor using TipTap
//! with wry-bindgen bindings.
//!
//! Adapted from https://github.com/lpotthast/leptos-tiptap

use wasm_bindgen::prelude::*;
use wasm_bindgen::{Closure, JsValue};
use web_sys::window;

// Use the tiptap bundle from leptos-tiptap-build
use leptos_tiptap_build::TIPTAP_BUNDLE_MIN_JS as TIPTAP_BUNDLE;

// TipTap wrapper module - adapted from leptos-tiptap
#[wasm_bindgen(crate = wasm_bindgen, inline_js = r#"
// Editor registry
window._tiptapEditors = new Map();

function _setEditor(id, editor, onSelection) {
  window._tiptapEditors.set(id, { editor, onSelection });
}

function _forgetEditor(id) {
  window._tiptapEditors.delete(id);
}

function _getEditor(id) {
  return window._tiptapEditors.get(id);
}

function _getSelectionState(editor) {
  return {
    h1: editor.isActive('heading', { level: 1 }),
    h2: editor.isActive('heading', { level: 2 }),
    h3: editor.isActive('heading', { level: 3 }),
    h4: editor.isActive('heading', { level: 4 }),
    h5: editor.isActive('heading', { level: 5 }),
    h6: editor.isActive('heading', { level: 6 }),
    paragraph: editor.isActive('paragraph'),
    bold: editor.isActive('bold'),
    italic: editor.isActive('italic'),
    strike: editor.isActive('strike'),
    blockquote: editor.isActive('blockquote'),
    highlight: editor.isActive('highlight'),
    bullet_list: editor.isActive('bulletList'),
    ordered_list: editor.isActive('orderedList'),
    align_left: editor.isActive({ textAlign: 'left' }),
    align_center: editor.isActive({ textAlign: 'center' }),
    align_right: editor.isActive({ textAlign: 'right' }),
    align_justify: editor.isActive({ textAlign: 'justify' }),
    link: editor.isActive('link'),
    youtube: editor.isActive('youtube'),
  };
}

export function create(id, content, editable, onChange, onSelection) {
  var myElem = document.getElementById(id);
  if (myElem == null) {
    console.error('Cannot create TipTap instance on element with id "' + id + '"');
    return;
  }

  var editor = new window.TipTap.Editor({
    element: myElem,
    editable: editable,
    extensions: [
      window.TipTapStarterKit.StarterKit,
      window.TipTapTextAlign.TextAlign.configure({
        types: ['heading', 'paragraph'],
      }),
      window.TipTapHighlight.Highlight,
      window.TipTapImage.Image,
      window.TipTapLink.Link,
      window.TipTapYoutube.Youtube
    ],
    injectCSS: false,
    content: content,
    onUpdate: ({ editor }) => {
      const html = editor.getHTML();
      onChange(html);
    },
    onSelectionUpdate: ({ editor }) => {
      onSelection(_getSelectionState(editor));
    },
  });

  _setEditor(id, editor, onSelection);
}

export function destroy(id) {
  const editorWindow = _getEditor(id);
  if (editorWindow && editorWindow.editor) {
    editorWindow.editor.destroy();
    _forgetEditor(id);
  }
}

export function getHTML(id) {
  const editorData = _getEditor(id);
  if (!editorData) return '';
  return editorData.editor.getHTML();
}

export function setEditable(id, editable) {
  const editorData = _getEditor(id);
  if (editorData) {
    editorData.editor.setEditable(editable);
  }
}

export function toggleHeading(id, level) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleHeading({ level: level }).run();
  onSelection(_getSelectionState(editor));
}

export function setParagraph(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setParagraph().run();
  onSelection(_getSelectionState(editor));
}

export function toggleBold(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleBold().run();
  onSelection(_getSelectionState(editor));
}

export function toggleItalic(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleItalic().run();
  onSelection(_getSelectionState(editor));
}

export function toggleStrike(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleStrike().run();
  onSelection(_getSelectionState(editor));
}

export function toggleBlockquote(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleBlockquote().run();
  onSelection(_getSelectionState(editor));
}

export function toggleHighlight(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleHighlight().run();
  onSelection(_getSelectionState(editor));
}

export function toggleBulletList(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleBulletList().run();
  onSelection(_getSelectionState(editor));
}

export function toggleOrderedList(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleOrderedList().run();
  onSelection(_getSelectionState(editor));
}

export function setTextAlignLeft(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setTextAlign('left').run();
  onSelection(_getSelectionState(editor));
}

export function setTextAlignCenter(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setTextAlign('center').run();
  onSelection(_getSelectionState(editor));
}

export function setTextAlignRight(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setTextAlign('right').run();
  onSelection(_getSelectionState(editor));
}

export function setTextAlignJustify(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setTextAlign('justify').run();
  onSelection(_getSelectionState(editor));
}

export function setImage(id, src, alt, title) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setImage({ src: src, alt: alt, title: title }).run();
  onSelection(_getSelectionState(editor));
}

export function setLink(id, href, target, rel) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setLink({ href: href, target: target, rel: rel }).run();
  onSelection(_getSelectionState(editor));
}

export function toggleLink(id, href, target, rel) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().toggleLink({ href: href, target: target, rel: rel }).run();
  onSelection(_getSelectionState(editor));
}

export function unsetLink(id) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().unsetLink().run();
  onSelection(_getSelectionState(editor));
}

export function setYoutubeVideo(id, src, start, width, height) {
  const { editor, onSelection } = _getEditor(id);
  editor.chain().focus().setYoutubeVideo({ src: src, start: start, width: width, height: height }).run();
  onSelection(_getSelectionState(editor));
}

export function injectBundle(js) {
  const script = document.createElement('script');
  script.textContent = js;
  document.head.appendChild(script);
}
"#)]
extern "C" {
    /// Create a new TipTap editor instance
    pub fn create(
        id: &str,
        content: &str,
        editable: bool,
        on_change: &Closure<dyn Fn(String)>,
        on_selection: &Closure<dyn Fn(JsValue)>,
    );

    /// Destroy an editor instance
    pub fn destroy(id: &str);

    /// Get the current HTML content
    #[wasm_bindgen(js_name = "getHTML")]
    pub fn get_html(id: &str) -> String;

    /// Set editor editability
    #[wasm_bindgen(js_name = "setEditable")]
    pub fn set_editable(id: &str, editable: bool);

    /// Toggle heading level (1-6)
    #[wasm_bindgen(js_name = "toggleHeading")]
    pub fn toggle_heading(id: &str, level: i32);

    /// Set to paragraph
    #[wasm_bindgen(js_name = "setParagraph")]
    pub fn set_paragraph(id: &str);

    /// Toggle bold formatting
    #[wasm_bindgen(js_name = "toggleBold")]
    pub fn toggle_bold(id: &str);

    /// Toggle italic formatting
    #[wasm_bindgen(js_name = "toggleItalic")]
    pub fn toggle_italic(id: &str);

    /// Toggle strikethrough formatting
    #[wasm_bindgen(js_name = "toggleStrike")]
    pub fn toggle_strike(id: &str);

    /// Toggle blockquote
    #[wasm_bindgen(js_name = "toggleBlockquote")]
    pub fn toggle_blockquote(id: &str);

    /// Toggle highlight
    #[wasm_bindgen(js_name = "toggleHighlight")]
    pub fn toggle_highlight(id: &str);

    /// Toggle bullet list
    #[wasm_bindgen(js_name = "toggleBulletList")]
    pub fn toggle_bullet_list(id: &str);

    /// Toggle ordered list
    #[wasm_bindgen(js_name = "toggleOrderedList")]
    pub fn toggle_ordered_list(id: &str);

    /// Set text alignment to left
    #[wasm_bindgen(js_name = "setTextAlignLeft")]
    pub fn set_text_align_left(id: &str);

    /// Set text alignment to center
    #[wasm_bindgen(js_name = "setTextAlignCenter")]
    pub fn set_text_align_center(id: &str);

    /// Set text alignment to right
    #[wasm_bindgen(js_name = "setTextAlignRight")]
    pub fn set_text_align_right(id: &str);

    /// Set text alignment to justify
    #[wasm_bindgen(js_name = "setTextAlignJustify")]
    pub fn set_text_align_justify(id: &str);

    /// Insert an image
    #[wasm_bindgen(js_name = "setImage")]
    pub fn set_image(id: &str, src: &str, alt: &str, title: &str);

    /// Set a link on the selection
    #[wasm_bindgen(js_name = "setLink")]
    pub fn set_link(id: &str, href: &str, target: &str, rel: &str);

    /// Toggle a link on the selection
    #[wasm_bindgen(js_name = "toggleLink")]
    pub fn toggle_link(id: &str, href: &str, target: &str, rel: &str);

    /// Remove a link from the selection
    #[wasm_bindgen(js_name = "unsetLink")]
    pub fn unset_link(id: &str);

    /// Insert a YouTube video
    #[wasm_bindgen(js_name = "setYoutubeVideo")]
    pub fn set_youtube_video(id: &str, src: &str, start: &str, width: &str, height: &str);

    /// Inject the TipTap bundle script
    #[wasm_bindgen(js_name = "injectBundle")]
    pub fn inject_bundle(js: &str);
}

/// Selection state from the editor
#[derive(Default, Debug, Clone)]
pub struct SelectionState {
    pub h1: bool,
    pub h2: bool,
    pub h3: bool,
    pub h4: bool,
    pub h5: bool,
    pub h6: bool,
    pub paragraph: bool,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub blockquote: bool,
    pub highlight: bool,
    pub bullet_list: bool,
    pub ordered_list: bool,
    pub align_left: bool,
    pub align_center: bool,
    pub align_right: bool,
    pub align_justify: bool,
    pub link: bool,
    pub youtube: bool,
}

impl SelectionState {
    /// Parse selection state from a JsValue
    pub fn from_js(value: &JsValue) -> Self {
        #[wasm_bindgen(crate = wasm_bindgen, inline_js = r#"
export function getBool(obj, key) {
    return obj && obj[key] === true;
}
"#)]
        extern "C" {
            #[wasm_bindgen(js_name = "getBool")]
            fn get_bool(obj: &JsValue, key: &str) -> bool;
        }

        SelectionState {
            h1: get_bool(value, "h1"),
            h2: get_bool(value, "h2"),
            h3: get_bool(value, "h3"),
            h4: get_bool(value, "h4"),
            h5: get_bool(value, "h5"),
            h6: get_bool(value, "h6"),
            paragraph: get_bool(value, "paragraph"),
            bold: get_bool(value, "bold"),
            italic: get_bool(value, "italic"),
            strike: get_bool(value, "strike"),
            blockquote: get_bool(value, "blockquote"),
            highlight: get_bool(value, "highlight"),
            bullet_list: get_bool(value, "bullet_list"),
            ordered_list: get_bool(value, "ordered_list"),
            align_left: get_bool(value, "align_left"),
            align_center: get_bool(value, "align_center"),
            align_right: get_bool(value, "align_right"),
            align_justify: get_bool(value, "align_justify"),
            link: get_bool(value, "link"),
            youtube: get_bool(value, "youtube"),
        }
    }
}

/// CSS styles for the editor
const EDITOR_STYLES: &str = r#"
:root {
    --editor-bg: #ffffff;
    --editor-border: #e0e0e0;
    --toolbar-bg: #f5f5f5;
    --button-active: #007bff;
    --button-hover: #e0e0e0;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    margin: 0;
    padding: 20px;
    background: #f0f0f0;
}

.editor-container {
    max-width: 800px;
    margin: 0 auto;
    background: var(--editor-bg);
    border-radius: 8px;
    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
    overflow: hidden;
}

.toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 8px;
    background: var(--toolbar-bg);
    border-bottom: 1px solid var(--editor-border);
}

.toolbar-group {
    display: flex;
    gap: 2px;
    padding-right: 8px;
    border-right: 1px solid var(--editor-border);
    margin-right: 4px;
}

.toolbar-group:last-child {
    border-right: none;
}

.toolbar button {
    padding: 6px 10px;
    border: none;
    background: transparent;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 500;
    color: #333;
    transition: background-color 0.2s;
}

.toolbar button:hover {
    background: var(--button-hover);
}

.toolbar button.active {
    background: var(--button-active);
    color: white;
}

#editor {
    min-height: 300px;
    padding: 16px;
}

#editor:focus {
    outline: none;
}

/* TipTap editor content styles */
.ProseMirror {
    outline: none;
    white-space: pre-wrap;
}

.ProseMirror p {
    margin: 0 0 1em 0;
}

.ProseMirror h1 { font-size: 2em; margin: 0.67em 0; }
.ProseMirror h2 { font-size: 1.5em; margin: 0.75em 0; }
.ProseMirror h3 { font-size: 1.17em; margin: 0.83em 0; }
.ProseMirror h4 { font-size: 1em; margin: 1.12em 0; }
.ProseMirror h5 { font-size: 0.83em; margin: 1.5em 0; }
.ProseMirror h6 { font-size: 0.75em; margin: 1.67em 0; }

.ProseMirror blockquote {
    border-left: 3px solid #ccc;
    margin: 1em 0;
    padding-left: 1em;
    color: #666;
}

.ProseMirror ul, .ProseMirror ol {
    margin: 1em 0;
    padding-left: 2em;
}

.ProseMirror mark {
    background-color: #ffc107;
}

.ProseMirror a {
    color: var(--button-active);
    text-decoration: underline;
}

.ProseMirror img {
    max-width: 100%;
    height: auto;
}

.ProseMirror iframe {
    max-width: 100%;
}

#output {
    margin-top: 20px;
    padding: 16px;
    background: white;
    border-radius: 8px;
    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
}

#output h3 {
    margin-top: 0;
    color: #333;
}

#output pre {
    background: #f5f5f5;
    padding: 12px;
    border-radius: 4px;
    overflow-x: auto;
    white-space: pre-wrap;
    word-wrap: break-word;
}
"#;

/// HTML template for the editor
const EDITOR_HTML: &str = r#"
<div class="editor-container">
    <div class="toolbar" id="toolbar">
        <div class="toolbar-group">
            <button data-action="h1">H1</button>
            <button data-action="h2">H2</button>
            <button data-action="h3">H3</button>
            <button data-action="paragraph">P</button>
        </div>
        <div class="toolbar-group">
            <button data-action="bold"><b>B</b></button>
            <button data-action="italic"><i>I</i></button>
            <button data-action="strike"><s>S</s></button>
            <button data-action="highlight">H</button>
        </div>
        <div class="toolbar-group">
            <button data-action="bulletList">&#8226; List</button>
            <button data-action="orderedList">1. List</button>
            <button data-action="blockquote">&ldquo;</button>
        </div>
        <div class="toolbar-group">
            <button data-action="alignLeft">&#8676;</button>
            <button data-action="alignCenter">&#8596;</button>
            <button data-action="alignRight">&#8677;</button>
        </div>
    </div>
    <div id="editor"></div>
</div>
<div id="output">
    <h3>HTML Output</h3>
    <pre id="html-output"></pre>
</div>
"#;

fn main() {
    wry_testing::run(|| async {
        let document = window().unwrap().document().unwrap();
        let body = document.body().unwrap();

        // Add styles
        let style = document.create_element("style").unwrap();
        style.set_inner_html(EDITOR_STYLES);
        document.head().unwrap().append_child(&style).unwrap();

        // Inject TipTap bundle
        inject_bundle(TIPTAP_BUNDLE);

        // Add editor HTML
        body.set_inner_html(EDITOR_HTML);

        // Initial content
        let initial_content = "<p>Welcome to the <strong>TipTap</strong> editor!</p><p>Try formatting some text using the toolbar above.</p>";

        // Create callbacks for content and selection changes
        let on_change = Closure::new(|html: String| {
            if let Some(output) = window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("html-output"))
            {
                output.set_inner_html(&html);
            }
        });

        let on_selection = Closure::new(|selection: JsValue| {
            let state = SelectionState::from_js(&selection);

            // Update toolbar button states
            if let Some(document) = window().and_then(|w| w.document()) {
                let update_button = |action: &str, active: bool| {
                    if let Some(btn) = document.query_selector(&format!("[data-action='{action}']")).ok().flatten() {
                        if active {
                            let _ = btn.class_list().add_1("active");
                        } else {
                            let _ = btn.class_list().remove_1("active");
                        }
                    }
                };

                update_button("h1", state.h1);
                update_button("h2", state.h2);
                update_button("h3", state.h3);
                update_button("paragraph", state.paragraph);
                update_button("bold", state.bold);
                update_button("italic", state.italic);
                update_button("strike", state.strike);
                update_button("highlight", state.highlight);
                update_button("bulletList", state.bullet_list);
                update_button("orderedList", state.ordered_list);
                update_button("blockquote", state.blockquote);
                update_button("alignLeft", state.align_left);
                update_button("alignCenter", state.align_center);
                update_button("alignRight", state.align_right);
            }
        });

        // Create the editor
        create("editor", initial_content, true, &on_change, &on_selection);

        // Set initial HTML output
        if let Some(output) = document.get_element_by_id("html-output") {
            output.set_inner_html(initial_content);
        }

        // Set up toolbar click handlers
        setup_toolbar_handlers();

        // Keep the application running
        std::future::pending::<()>().await;
    })
    .unwrap();
}

fn setup_toolbar_handlers() {
    #[wasm_bindgen(crate = wasm_bindgen, inline_js = r#"
export function setupToolbar(editorId) {
    const toolbar = document.getElementById('toolbar');
    if (!toolbar) return;

    toolbar.addEventListener('click', (e) => {
        const button = e.target.closest('button');
        if (!button) return;

        const action = button.dataset.action;
        if (!action) return;

        const editorData = window._tiptapEditors.get(editorId);
        if (!editorData) return;

        const { editor, onSelection } = editorData;

        const getSelectionState = (editor) => ({
            h1: editor.isActive('heading', { level: 1 }),
            h2: editor.isActive('heading', { level: 2 }),
            h3: editor.isActive('heading', { level: 3 }),
            h4: editor.isActive('heading', { level: 4 }),
            h5: editor.isActive('heading', { level: 5 }),
            h6: editor.isActive('heading', { level: 6 }),
            paragraph: editor.isActive('paragraph'),
            bold: editor.isActive('bold'),
            italic: editor.isActive('italic'),
            strike: editor.isActive('strike'),
            blockquote: editor.isActive('blockquote'),
            highlight: editor.isActive('highlight'),
            bullet_list: editor.isActive('bulletList'),
            ordered_list: editor.isActive('orderedList'),
            align_left: editor.isActive({ textAlign: 'left' }),
            align_center: editor.isActive({ textAlign: 'center' }),
            align_right: editor.isActive({ textAlign: 'right' }),
            align_justify: editor.isActive({ textAlign: 'justify' }),
            link: editor.isActive('link'),
            youtube: editor.isActive('youtube'),
        });

        switch(action) {
            case 'h1':
                editor.chain().focus().toggleHeading({ level: 1 }).run();
                break;
            case 'h2':
                editor.chain().focus().toggleHeading({ level: 2 }).run();
                break;
            case 'h3':
                editor.chain().focus().toggleHeading({ level: 3 }).run();
                break;
            case 'paragraph':
                editor.chain().focus().setParagraph().run();
                break;
            case 'bold':
                editor.chain().focus().toggleBold().run();
                break;
            case 'italic':
                editor.chain().focus().toggleItalic().run();
                break;
            case 'strike':
                editor.chain().focus().toggleStrike().run();
                break;
            case 'highlight':
                editor.chain().focus().toggleHighlight().run();
                break;
            case 'bulletList':
                editor.chain().focus().toggleBulletList().run();
                break;
            case 'orderedList':
                editor.chain().focus().toggleOrderedList().run();
                break;
            case 'blockquote':
                editor.chain().focus().toggleBlockquote().run();
                break;
            case 'alignLeft':
                editor.chain().focus().setTextAlign('left').run();
                break;
            case 'alignCenter':
                editor.chain().focus().setTextAlign('center').run();
                break;
            case 'alignRight':
                editor.chain().focus().setTextAlign('right').run();
                break;
        }

        onSelection(getSelectionState(editor));
    });
}
"#)]
    extern "C" {
        #[wasm_bindgen(js_name = "setupToolbar")]
        fn setup_toolbar(editor_id: &str);
    }

    setup_toolbar("editor");
}
