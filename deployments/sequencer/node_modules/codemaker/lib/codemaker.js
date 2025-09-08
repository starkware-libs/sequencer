"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.CodeMaker = void 0;
const util = require("util");
const caseutils = require("./case-utils");
const filebuff_1 = require("./filebuff");
/**
 * Multi-file text writer with some code-generation features.
 */
class CodeMaker {
    constructor({ indentationLevel = 4, indentCharacter = ' ', } = {}) {
        this.currIndent = 0;
        this.files = new Array();
        this.excludes = new Array();
        /**
         * Formats an block open statement.
         */
        this.openBlockFormatter = (s) => `${s} {`;
        /**
         * Formats a block close statement.
         */
        this.closeBlockFormatter = () => '}';
        this.indentation = indentationLevel;
        this.indentCharacter = indentCharacter;
    }
    get currentIndentLength() {
        return this.currIndent * this.indentation;
    }
    /**
     * Saves all the files created in this code maker.
     * @param rootDir The root directory for all saved files.
     * @returns A sorted list of all the files saved (absolute paths).
     */
    async save(rootDir) {
        const paths = this.files
            .filter((file) => !this.excludes.includes(file.filePath))
            .map((file) => file.save(rootDir));
        return (await Promise.all(paths)).sort();
    }
    /**
     * Sets the name of the current file we are working with.
     * Note that this doesn't really create a new file (files are only created when save() is called.
     * Use `closeFile` to close this file.
     * @param filePath The relative path of the new file.
     */
    openFile(filePath) {
        if (this.currentFile) {
            throw new Error(`Cannot open file ${filePath} without closing the previous file ${this.currentFile.filePath}`);
        }
        this.currentFile = new filebuff_1.default(filePath);
    }
    /**
     * Indicates that we finished generating the current file.
     * @param filePath The relative file path (must be the same as one passed to openFile)
     */
    closeFile(filePath) {
        if (!this.currentFile) {
            throw new Error(`Cannot close file ${filePath}. It was never opened`);
        }
        if (this.currentFile.filePath !== filePath) {
            throw new Error(`Cannot close file ${filePath}. The currently opened file is ${this.currentFile.filePath}`);
        }
        this.files.push(this.currentFile);
        this.currentFile = undefined;
    }
    /**
     * Emits a line into the currently opened file.
     * Line is emitted with the current level of indentation.
     * If no arguments are provided, an empty new line is emitted.
     * @param fmt String format arguments (passed to `util.format`)
     * @param args String arguments
     */
    line(fmt, ...args) {
        if (!this.currentFile) {
            throw new Error('Cannot emit source lines without opening a file');
        }
        if (fmt) {
            fmt = this.makeIndent() + fmt;
            this.currentFile.write(util.format(fmt, ...args));
        }
        this.currentFile.write('\n');
    }
    /**
     * Same as `open`.
     */
    indent(textBefore) {
        this.open(textBefore);
    }
    /**
     * Same as `close`.
     */
    unindent(textAfter) {
        this.close(textAfter);
    }
    /**
     * Increases the indentation level by `indentation` spaces for the next line.
     * @param textBefore Text to emit before the newline (i.e. block open).
     */
    open(textBefore) {
        this.line(textBefore);
        this.currIndent++;
    }
    /**
     * Decreases the indentation level by `indentation` for the next line.
     * @param textAfter Text to emit in the line after indentation was decreased.
     *                  If `false` no line will be emitted at all, but the indent
     *                  counter will be decremented.
     */
    close(textAfter) {
        this.currIndent--;
        if (textAfter !== false) {
            this.line(textAfter);
        }
    }
    /**
     * Opens a code block. The formatting of the block is determined by `openBlockFormatter`.
     * @param text The text to pass to the formatter.
     */
    openBlock(text) {
        this.open(this.openBlockFormatter(text));
    }
    /**
     * Closes a code block. The formatting of the block is determined by `closeBlockFormatter`.
     * @param text The text to pass to the formatter.
     */
    closeBlock(text) {
        this.close(this.closeBlockFormatter(text));
    }
    /**
     * Adds a file to the exclude list. This means this file will not be saved during save().
     * @param filePath The relative path of the file.
     */
    exclude(filePath) {
        this.excludes.push(filePath);
    }
    /**
     * convertsStringToCamelCase
     */
    toCamelCase(...args) {
        return caseutils.toCamelCase(...args);
    }
    /**
     * ConvertsStringToPascalCase
     */
    toPascalCase(...args) {
        return caseutils.toPascalCase(...args);
    }
    /**
     * convert_string_to_snake_case
     * @param sep Separator (defaults to '_')
     */
    toSnakeCase(s, sep = '_') {
        return caseutils.toSnakeCase(s, sep);
    }
    /**
     * Gets currently opened file path.
     * @returns Currently opened file path.
     */
    getCurrentFilePath() {
        return this.currentFile?.filePath;
    }
    makeIndent() {
        const length = this.currentIndentLength;
        if (length <= 0) {
            return '';
        }
        return this.indentCharacter.repeat(length);
    }
}
exports.CodeMaker = CodeMaker;
//# sourceMappingURL=codemaker.js.map