import * as spec from '@jsii/spec';
import { TypeScriptSnippet } from './snippet';
import { LanguageTablet, TranslatedSnippet } from './tablets/tablets';
import { TranslateAllResult } from './translate_all';
export interface RosettaTranslatorOptions {
    /**
     * Assemblies to use for fingerprinting
     *
     * The set of assemblies here are used to invalidate the cache. Any types that are
     * used in snippets are looked up in this set of assemblies. If found, their type
     * information is fingerprinted and compared to the type information at the time
     * compilation of the cached sample. If different, this is considered to be a cache
     * miss.
     *
     * You must use the same set of assemblies when generating and reading the cache
     * file, otherwise the fingerprint is guaranteed to be different and the cache will
     * be useless (e.g. if you generate the cache WITH assembly information but
     * read it without, or vice versa).
     *
     * @default No assemblies.
     */
    readonly assemblies?: spec.Assembly[];
    /**
     * Whether to include compiler diagnostics in the compilation results.
     *
     * @default false
     */
    readonly includeCompilerDiagnostics?: boolean;
    /**
     * Allow reading dirty translations from cache
     *
     * @default false
     */
    readonly allowDirtyTranslations?: boolean;
}
/**
 * Entry point for consumers that want to translate code on-the-fly
 *
 * If you want to generate and translate code on-the-fly, in ways that cannot
 * be achieved by the rosetta CLI, use this class.
 */
export declare class RosettaTranslator {
    /**
     * Tablet with fresh translations
     *
     * All new translations (not read from cache) are added to this tablet.
     */
    readonly tablet: LanguageTablet;
    readonly cache: LanguageTablet;
    private readonly fingerprinter;
    private readonly includeCompilerDiagnostics;
    private readonly allowDirtyTranslations;
    constructor(options?: RosettaTranslatorOptions);
    /**
     * @deprecated use `addToCache` instead
     */
    loadCache(fileName: string): Promise<void>;
    addToCache(filename: string): Promise<void>;
    addTabletsToCache(...tablets: LanguageTablet[]): void;
    hasCache(): boolean;
    /**
     * For all the given snippets, try to read translations from the cache
     *
     * Will remove the cached snippets from the input array.
     */
    readFromCache(snippets: TypeScriptSnippet[], addToTablet?: boolean, compiledOnly?: boolean): ReadFromCacheResults;
    translateAll(snippets: TypeScriptSnippet[], addToTablet?: boolean): Promise<TranslateAllResult>;
    translateAll(snippets: TypeScriptSnippet[], options?: TranslateAllOptions): Promise<TranslateAllResult>;
}
export type CacheHit = {
    readonly type: 'miss';
} | {
    readonly type: 'hit';
    readonly snippet: TranslatedSnippet;
    readonly infused: boolean;
} | {
    readonly type: 'dirty';
    readonly translation: TranslatedSnippet;
    readonly dirtySource: boolean;
    readonly dirtyTranslator: boolean;
    readonly dirtyTypes: boolean;
    readonly dirtyDidntCompile: boolean;
};
export interface ReadFromCacheResults {
    /**
     * Successful translations
     */
    readonly translations: TranslatedSnippet[];
    /**
     * Successful but dirty hits
     */
    readonly remaining: TypeScriptSnippet[];
    /**
     * How many successfully hit translations were infused
     */
    readonly infusedCount: number;
    readonly dirtyCount: number;
    readonly dirtySourceCount: number;
    readonly dirtyTranslatorCount: number;
    readonly dirtyTypesCount: number;
    readonly dirtyDidntCompile: number;
}
export interface TranslateAllOptions {
    /**
     * @default - Create a temporary directory with all necessary packages
     */
    readonly compilationDirectory?: string;
    /**
     * @default true
     */
    readonly addToTablet?: boolean;
    /**
     * @default true
     */
    readonly cleanup?: boolean;
}
//# sourceMappingURL=rosetta-translator.d.ts.map