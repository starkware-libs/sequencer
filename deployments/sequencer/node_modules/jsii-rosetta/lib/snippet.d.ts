/**
 * A piece of TypeScript code found in an assembly, ready to be translated
 */
export interface TypeScriptSnippet {
    /**
     * The snippet code that ends up in the JSII assembly
     */
    readonly visibleSource: string;
    /**
     * Description of where the snippet was found
     */
    readonly location: SnippetLocation;
    /**
     * When enhanced with a fixture, the snippet's complete source code
     */
    readonly completeSource?: string;
    /**
     * Parameters for the conversion
     */
    readonly parameters?: Record<string, string>;
    /**
     * Whether this snippet must be processed as if `--strict` was always supplied.
     *
     * @default false
     */
    readonly strict?: boolean;
    /**
     * Dependencies necessary to compile this snippet
     *
     * Value is a regular { name -> semver } map like NPM's `dependencies`,
     * `devDependencies` etc.
     *
     * @default none
     */
    readonly compilationDependencies?: Record<string, CompilationDependency>;
}
export type CompilationDependency = {
    readonly type: 'concrete';
    readonly resolvedDirectory: string;
} | {
    readonly type: 'symbolic';
    readonly versionRange: string;
};
/**
 * Description of a location where the snippet is found
 *
 * The location does not necessarily indicate an exact source file,
 * but it will generally refer to a location that can contain one or more
 * snippets.
 */
export interface SnippetLocation {
    /**
     * The jsii API with which this snippet is associated
     */
    readonly api: ApiLocation;
    /**
     * The API field in which the snippet is found, if any
     *
     * Absence of this field is appropriate for source files (or tests),
     * but for Markdown files 'field' should really be set to a Markdown
     * location.
     */
    readonly field?: FieldLocation;
}
/**
 * How to represent the initializer in a 'parameter' type.
 *
 * (Don't feel like making everyone's `case` statement worse by adding an
 * 'initializer-parameter' variant).
 */
export declare const INITIALIZER_METHOD_NAME = "<initializer>";
export type ApiLocation = {
    readonly api: 'file';
    readonly fileName: string;
} | {
    readonly api: 'moduleReadme';
    readonly moduleFqn: string;
} | {
    readonly api: 'type';
    readonly fqn: string;
} | {
    readonly api: 'initializer';
    readonly fqn: string;
} | {
    readonly api: 'member';
    readonly fqn: string;
    readonly memberName: string;
} | {
    readonly api: 'parameter';
    readonly fqn: string;
    readonly methodName: string | typeof INITIALIZER_METHOD_NAME;
    readonly parameterName: string;
};
export type FieldLocation = {
    readonly field: 'markdown';
    readonly line: number;
} | {
    readonly field: 'example';
};
/**
 * Render an API location to a human readable representation
 */
export declare function formatLocation(location: SnippetLocation): string;
/**
 * Render an API location to an unique string
 *
 * This function is used in hashing examples for reuse, and so the formatting
 * here should not be changed lightly.
 */
export declare function renderApiLocation(apiLoc: ApiLocation): string;
/**
 * Construct a TypeScript snippet from visible source
 *
 * Will parse parameters from a directive in the given source, but will not
 * interpret `/// !show` and `/// !hide` directives.
 *
 * `/// !show` and `/// !hide` directives WILL affect what gets displayed by
 * the translator, but they will NOT affect the snippet's cache key (i.e. the
 * cache key will be based on the full source given here).
 *
 * Use this if you are looking up a snippet in a tablet, which has been translated
 * previously using a fixture.
 */
export declare function typeScriptSnippetFromVisibleSource(typeScriptSource: string, location: SnippetLocation, strict: boolean, parameters?: Record<string, string>): TypeScriptSnippet;
/**
 * Construct a TypeScript snippet from literal source
 *
 * @deprecated Use `typeScriptSnippetFromVisibleSource`
 */
export declare function typeScriptSnippetFromSource(typeScriptSource: string, location: SnippetLocation, strict: boolean, parameters?: Record<string, string>): TypeScriptSnippet;
/**
 * Construct a TypeScript snippet from complete source
 *
 * Will parse parameters from a directive in the given source, and will
 * interpret `/// !show` and `/// !hide` directives.
 *
 * The snippet's cache key will be based on the source that remains after
 * these directives are processed.
 *
 * Use this if you are building a snippet to be translated, and take care
 * to store the return object's `visibleSource` in the assembly (not the original
 * source you passed in).
 */
export declare function typeScriptSnippetFromCompleteSource(typeScriptSource: string, location: SnippetLocation, strict: boolean, parameters?: Record<string, string>): TypeScriptSnippet;
export declare function updateParameters(snippet: TypeScriptSnippet, params: Record<string, string>): TypeScriptSnippet;
/**
 * Get the complete (compilable) source of a snippet
 */
export declare function completeSource(snippet: TypeScriptSnippet): string;
/**
 * Parse a set of 'param param=value' directives into an object
 */
export declare function parseKeyValueList(parameters: string[]): Record<string, string>;
export declare function parseMetadataLine(metadata: string): Record<string, string>;
export declare function renderMetadataline(metadata?: Record<string, string>): string | undefined;
/**
 * Recognized snippet parameters
 */
export declare enum SnippetParameters {
    /**
     * Use fixture with the given name (author parameter)
     */
    FIXTURE = "fixture",
    /**
     * Don't use a fixture (author parameter)
     */
    NO_FIXTURE = "nofixture",
    /**
     * Snippet was extracted from this literate file (backwards compatibility)
     *
     * Parameter attached by 'jsii'; load the given file instead of any fixture,
     * process as usual.
     */
    LITERATE_SOURCE = "lit",
    /**
     * This snippet has been infused
     *
     * This means it has been copied from a different location, and potentially
     * even from a different assembly. If so, we can't expect it to compile in
     * the future, and if doesn't, we ignore the errors.
     *
     * N.B: this shouldn't make a difference in normal operation, as the `infuse`
     * command will duplicate the translation to the target tablet. This only
     * matters if we remove the tablet and try to re-extract an assembly with
     * infused examples from somewher else.
     */
    INFUSED = "infused",
    /**
     * What directory to resolve fixtures in for this snippet (system parameter)
     *
     * Attached during processing, should not be used by authors. Does NOT imply
     * anything about the directory where we pretend to compile this file.
     */
    $PROJECT_DIRECTORY = "$directory",
    /**
     * What directory to pretend the file is in (system parameter)
     *
     * Attached when compiling a literate file, as they compile in
     * the location where they are stored.
     */
    $COMPILATION_DIRECTORY = "$compilation"
}
//# sourceMappingURL=snippet.d.ts.map