/**
 * Doc Comment parsing
 *
 * I tried using TSDoc here.
 *
 * Pro:
 * - Future standard.
 * - Does validating parsing and complains on failure.
 * - Has a more flexible interpretation of the @example tag (starts in text mode).
 *
 * Con:
 * - Different tags from JSDoc (@defaultValue instead of @default, "@param name
 *   description" instead "@param name description".
 * - @example tag has a different interpretation than VSCode and JSDoc
 *   (VSCode/JSDoc starts in code mode), which is confusing for syntax
 *   highlighting in the editor.
 * - Allows no unknown tags.
 * - Wants to be in charge of parsing TypeScript, integrating into other build is
 *   possible but harder.
 * - Parse to a DOM with no easy way to roundtrip back to equivalent MarkDown.
 *
 * Especially the last point: while parsing to and storing the parsed docs DOM
 * in the jsii assembly is superior in the long term (for example for
 * converting to different output formats, JavaDoc, C# docs, refdocs which all
 * accept slightly different syntaxes), right now we can get 80% of the bang
 * for 10% of the buck by just reading, storing and reproducing MarkDown, which
 * is Readable Enough(tm).
 *
 * If we ever want to attempt TSDoc again, this would be a good place to look at:
 *
 * https://github.com/Microsoft/tsdoc/blob/master/api-demo/src/advancedDemo.ts
 */
import * as spec from '@jsii/spec';
import * as ts from 'typescript';
/**
 * Parse all doc comments that apply to a symbol into JSIIDocs format
 */
export declare function parseSymbolDocumentation(sym: ts.Symbol, typeChecker: ts.TypeChecker): DocsParsingResult;
/**
 * Return the list of parameter names that are referenced in the docstring for this symbol
 */
export declare function getReferencedDocParams(sym: ts.Symbol): string[];
export interface DocsParsingResult {
    docs: spec.Docs;
    hints: TypeSystemHints;
    diagnostics?: string[];
}
export interface TypeSystemHints {
    /**
     * Only present on interfaces. This indicates that interface must be handled as a struct/data type
     * even through it's name starts with a capital letter `I`.
     */
    struct?: boolean;
}
/**
 * Split the doc comment into summary and remarks
 *
 * Normally, we'd expect people to split into a summary line and detail lines using paragraph
 * markers. However, a LOT of people do not do this, and just paste a giant comment block into
 * the docstring. If we detect that situation, we will try and extract the first sentence (using
 * a period) as the summary.
 */
export declare function splitSummary(docBlock: string | undefined): [string | undefined, string | undefined];
//# sourceMappingURL=docs.d.ts.map