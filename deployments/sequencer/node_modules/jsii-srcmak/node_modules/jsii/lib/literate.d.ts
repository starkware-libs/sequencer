/**
 * Convert an annotated TypeScript source file to MarkDown
 */
export declare function typescriptSourceToMarkdown(lines: string[], codeBlockAnnotations: string[]): string[];
export interface LoadedFile {
    readonly fullPath: string;
    readonly lines: string[];
}
export type FileLoader = (relativePath: string) => LoadedFile;
/**
 * Given MarkDown source, find source files to include and render
 *
 * We recognize links on a line by themselves if the link text starts
 * with the string "example" (case insensitive). For example:
 *
 *     [example](test/integ.bucket.ts)
 */
export declare function includeAndRenderExamples(lines: string[], loader: FileLoader, projectRoot: string): string[];
/**
 * Load a file into a string array
 */
export declare function loadFromFile(fileName: string): string[];
/**
 * Turn file content string into an array of lines ready for processing using the other functions
 */
export declare function contentToLines(content: string): string[];
/**
 * Return a file system loader given a base directory
 */
export declare function fileSystemLoader(directory: string): FileLoader;
//# sourceMappingURL=literate.d.ts.map