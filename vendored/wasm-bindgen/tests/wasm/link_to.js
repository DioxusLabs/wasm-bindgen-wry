const fs = require('fs');
const url = require('url');

export const read_file = (str) => fs.readFileSync(url.fileURLToPath(str), "utf8");
