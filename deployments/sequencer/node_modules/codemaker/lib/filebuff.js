"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const path = require("path");
/**
 * Buffers the text of a file for later saving.
 */
class FileBuffer {
    constructor(filePath) {
        this.buffer = '';
        this.filePath = filePath;
    }
    write(s) {
        this.buffer += s;
    }
    async save(rootDir) {
        // just-in-time require so that this file can be loaded in browsers as well.
        // eslint-disable-next-line @typescript-eslint/no-require-imports,@typescript-eslint/no-var-requires
        const fs = require('fs-extra');
        const fullPath = path.join(rootDir, this.filePath);
        await fs.mkdirs(path.dirname(fullPath));
        await fs.writeFile(fullPath, this.buffer);
        return fullPath;
    }
}
exports.default = FileBuffer;
//# sourceMappingURL=filebuff.js.map