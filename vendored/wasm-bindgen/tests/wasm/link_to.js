export const read_file = (str) => {
    const xhr = new XMLHttpRequest();
    xhr.open("GET", str, false);
    xhr.send();
    if (xhr.status < 200 || xhr.status >= 300) {
        throw new Error("read_file failed (" + xhr.status + "): " + str);
    }
    // A `link_to!` URL serves the module as JavaScript; an unresolved specifier
    // falls through to the host's root HTML page, which is not a module.
    const type = xhr.getResponseHeader("Content-Type") || "";
    if (!type.includes("javascript")) {
        throw new Error("read_file got non-module content (" + type + "): " + str);
    }
    return xhr.responseText;
};
