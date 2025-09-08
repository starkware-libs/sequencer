import * as ts from 'typescript';
import { TargetLanguage } from './languages';
import { AstHandler, AstRendererOptions } from './renderer';
import { TypeScriptSnippet } from './snippet';
import { TranslatedSnippet } from './tablets/tablets';
import { TypeScriptCompiler } from './typescript/ts-compiler';
import { File } from './util';
export declare function translateTypeScript(source: File, visitor: AstHandler<any>, options?: SnippetTranslatorOptions): TranslateResult;
/**
 * Translate one or more TypeScript snippets into other languages
 *
 * Can be configured to fully typecheck the samples, or perform only syntactical
 * translation.
 */
export declare class Translator {
    #private;
    private readonly includeCompilerDiagnostics;
    private readonly compiler;
    constructor(includeCompilerDiagnostics: boolean);
    translate(snip: TypeScriptSnippet, languages?: readonly TargetLanguage[]): TranslatedSnippet;
    get diagnostics(): readonly RosettaDiagnostic[];
    /**
     * Return the snippet translator for the given snippet
     *
     * We used to cache these, but each translator holds on to quite a bit of memory,
     * so we don't do that anymore.
     */
    translatorFor(snippet: TypeScriptSnippet): SnippetTranslator;
}
export interface SnippetTranslatorOptions extends AstRendererOptions {
    /**
     * Re-use the given compiler if given
     */
    readonly compiler?: TypeScriptCompiler;
    /**
     * Include compiler errors in return diagnostics
     *
     * If false, only translation diagnostics will be returned.
     *
     * @default false
     */
    readonly includeCompilerDiagnostics?: boolean;
}
export interface TranslateResult {
    translation: string;
    diagnostics: readonly RosettaDiagnostic[];
}
/**
 * A translation of a TypeScript diagnostic into a data-only representation for Rosetta
 *
 * We cannot use the original `ts.Diagnostic` since it holds on to way too much
 * state (the source file and by extension the entire parse tree), which grows
 * too big to be properly serialized by a worker and also takes too much memory.
 *
 * Reduce it down to only the information we need.
 */
export interface RosettaDiagnostic {
    /**
     * If this is an error diagnostic or not
     */
    readonly isError: boolean;
    /**
     * If the diagnostic was emitted from an assembly that has its 'strict' flag set
     */
    readonly isFromStrictAssembly: boolean;
    /**
     * The formatted message, ready to be printed (will have colors and newlines in it)
     *
     * Ends in a newline.
     */
    readonly formattedMessage: string;
}
export declare function makeRosettaDiagnostic(isError: boolean, formattedMessage: string): RosettaDiagnostic;
/**
 * Translate a single TypeScript snippet
 */
export declare class SnippetTranslator {
    private readonly options;
    readonly translateDiagnostics: ts.Diagnostic[];
    readonly compileDiagnostics: ts.Diagnostic[];
    private readonly visibleSpans;
    private readonly compilation;
    private readonly tryCompile;
    private readonly submoduleReferences;
    constructor(snippet: TypeScriptSnippet, options?: SnippetTranslatorOptions);
    /**
     * Returns a boolean if compilation was attempted, and undefined if it was not.
     */
    get didSuccessfullyCompile(): boolean | undefined;
    renderUsing(visitor: AstHandler<any>): string;
    syntaxKindCounter(): Partial<Record<ts.SyntaxKind, number>>;
    fqnsReferenced(): string[];
    get diagnostics(): readonly ts.Diagnostic[];
}
/**
 * Turn TypeScript diagnostics into Rosetta diagnostics
 */
export declare function rosettaDiagFromTypescript(diag: ts.Diagnostic): RosettaDiagnostic;
//# sourceMappingURL=translate.d.ts.map