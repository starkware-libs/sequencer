import { TranslatedSnippetSchema } from './schema';
import { TargetLanguage } from '../languages';
import { TypeScriptSnippet, SnippetLocation } from '../snippet';
import { Mutable } from '../util';
/**
 * The default name of the tablet file
 */
export declare const DEFAULT_TABLET_NAME = ".jsii.tabl.json";
/**
 * The default name of the compressed tablet file
 */
export declare const DEFAULT_TABLET_NAME_COMPRESSED = ".jsii.tabl.json.gz";
export declare const CURRENT_SCHEMA_VERSION = "2";
/**
 * A tablet containing various snippets in multiple languages
 */
export declare class LanguageTablet {
    /**
     * Load a tablet from a file
     */
    static fromFile(filename: string): Promise<LanguageTablet>;
    /**
     * Load a tablet from a file that may not exist
     *
     * Will return an empty tablet if the file does not exist
     */
    static fromOptionalFile(filename: string): Promise<LanguageTablet>;
    /**
     * Whether or not the LanguageTablet was loaded with a compressed source.
     * This gets used to determine if it should be compressed when saved.
     */
    compressedSource: boolean;
    private readonly snippets;
    /**
     * Add one or more snippets to this tablet
     */
    addSnippets(...snippets: TranslatedSnippet[]): void;
    /**
     * Add one snippet to this tablet
     *
     * @deprecated use addSnippets instead
     */
    addSnippet(snippet: TranslatedSnippet): void;
    get snippetKeys(): string[];
    /**
     * Add all snippets from the given tablets into this one
     */
    addTablets(...tablets: LanguageTablet[]): void;
    /**
     * Add all snippets from the given tablet into this one
     *
     * @deprecated Use `addTablets()` instead.
     */
    addTablet(tablet: LanguageTablet): void;
    tryGetSnippet(key: string): TranslatedSnippet | undefined;
    /**
     * Look up a single translation of a source snippet
     *
     * @deprecated Use `lookupTranslationBySource` instead.
     */
    lookup(typeScriptSource: TypeScriptSnippet, language: TargetLanguage): Translation | undefined;
    /**
     * Look up a single translation of a source snippet
     */
    lookupTranslationBySource(typeScriptSource: TypeScriptSnippet, language: TargetLanguage): Translation | undefined;
    /**
     * Lookup the translated verion of a TypeScript snippet
     */
    lookupBySource(typeScriptSource: TypeScriptSnippet): TranslatedSnippet | undefined;
    /**
     * Load the tablet from a file. Will automatically detect if the file is
     * compressed and decompress accordingly.
     */
    load(filename: string): Promise<void>;
    get count(): number;
    get translatedSnippets(): TranslatedSnippet[];
    /**
     * Saves the tablet schema to a file. If the compress option is passed, then
     * the schema will be gzipped before writing to the file.
     */
    save(filename: string, compress?: boolean): Promise<void>;
    private toSchema;
}
/**
 * Mutable operations on an underlying TranslatedSnippetSchema
 */
export declare class TranslatedSnippet {
    static fromSchema(schema: TranslatedSnippetSchema): TranslatedSnippet;
    static fromTypeScript(original: TypeScriptSnippet, didCompile?: boolean): TranslatedSnippet;
    readonly snippet: TranslatedSnippetSchema;
    private readonly _snippet;
    private _key?;
    private constructor();
    get key(): string;
    get originalSource(): Translation;
    addTranslation(language: TargetLanguage, translation: string, version: string): Translation;
    fqnsReferenced(): string[];
    addSyntaxKindCounter(syntaxKindCounter: Record<string, number>): void;
    get languages(): TargetLanguage[];
    get(language: TargetLanguage): Translation | undefined;
    mergeTranslations(other: TranslatedSnippet): TranslatedSnippet;
    withFingerprint(fp: string): TranslatedSnippet;
    withLocation(location: SnippetLocation): TranslatedSnippet;
    toJSON(): Mutable<TranslatedSnippetSchema>;
    private asTypescriptSnippet;
}
export interface Translation {
    source: string;
    language: string;
    didCompile?: boolean;
}
//# sourceMappingURL=tablets.d.ts.map