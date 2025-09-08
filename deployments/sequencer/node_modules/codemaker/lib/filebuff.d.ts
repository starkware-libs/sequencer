/**
 * Buffers the text of a file for later saving.
 */
export default class FileBuffer {
    readonly filePath: string;
    private buffer;
    constructor(filePath: string);
    write(s: string): void;
    save(rootDir: string): Promise<string>;
}
//# sourceMappingURL=filebuff.d.ts.map