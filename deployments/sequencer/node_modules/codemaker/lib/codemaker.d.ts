/**
 * Multi-file text writer with some code-generation features.
 */
export declare class CodeMaker {
    /**
     * The indentation level of the file.
     */
    indentation: number;
    /**
     * The character to use for indentation. When setting this to `\t`, consider
     * also setting `indentation` to `1`.
     */
    indentCharacter: ' ' | '\t';
    private currIndent;
    private currentFile?;
    private readonly files;
    private readonly excludes;
    constructor({ indentationLevel, indentCharacter, }?: {
        indentationLevel?: CodeMaker['indentation'];
        indentCharacter?: CodeMaker['indentCharacter'];
    });
    get currentIndentLength(): number;
    /**
     * Formats an block open statement.
     */
    openBlockFormatter: (s?: string) => string;
    /**
     * Formats a block close statement.
     */
    closeBlockFormatter: (s?: string) => string | false;
    /**
     * Saves all the files created in this code maker.
     * @param rootDir The root directory for all saved files.
     * @returns A sorted list of all the files saved (absolute paths).
     */
    save(rootDir: string): Promise<string[]>;
    /**
     * Sets the name of the current file we are working with.
     * Note that this doesn't really create a new file (files are only created when save() is called.
     * Use `closeFile` to close this file.
     * @param filePath The relative path of the new file.
     */
    openFile(filePath: string): void;
    /**
     * Indicates that we finished generating the current file.
     * @param filePath The relative file path (must be the same as one passed to openFile)
     */
    closeFile(filePath: string): void;
    /**
     * Emits a line into the currently opened file.
     * Line is emitted with the current level of indentation.
     * If no arguments are provided, an empty new line is emitted.
     * @param fmt String format arguments (passed to `util.format`)
     * @param args String arguments
     */
    line(fmt?: string, ...args: string[]): void;
    /**
     * Same as `open`.
     */
    indent(textBefore?: string): void;
    /**
     * Same as `close`.
     */
    unindent(textAfter?: string | false): void;
    /**
     * Increases the indentation level by `indentation` spaces for the next line.
     * @param textBefore Text to emit before the newline (i.e. block open).
     */
    open(textBefore?: string): void;
    /**
     * Decreases the indentation level by `indentation` for the next line.
     * @param textAfter Text to emit in the line after indentation was decreased.
     *                  If `false` no line will be emitted at all, but the indent
     *                  counter will be decremented.
     */
    close(textAfter?: string | false): void;
    /**
     * Opens a code block. The formatting of the block is determined by `openBlockFormatter`.
     * @param text The text to pass to the formatter.
     */
    openBlock(text: string): void;
    /**
     * Closes a code block. The formatting of the block is determined by `closeBlockFormatter`.
     * @param text The text to pass to the formatter.
     */
    closeBlock(text?: string): void;
    /**
     * Adds a file to the exclude list. This means this file will not be saved during save().
     * @param filePath The relative path of the file.
     */
    exclude(filePath: string): void;
    /**
     * convertsStringToCamelCase
     */
    toCamelCase(...args: string[]): string;
    /**
     * ConvertsStringToPascalCase
     */
    toPascalCase(...args: string[]): string;
    /**
     * convert_string_to_snake_case
     * @param sep Separator (defaults to '_')
     */
    toSnakeCase(s: string, sep?: string): string;
    /**
     * Gets currently opened file path.
     * @returns Currently opened file path.
     */
    getCurrentFilePath(): string | undefined;
    private makeIndent;
}
//# sourceMappingURL=codemaker.d.ts.map