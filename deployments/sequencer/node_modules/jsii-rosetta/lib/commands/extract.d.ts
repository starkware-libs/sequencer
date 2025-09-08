import { RosettaTranslator, RosettaTranslatorOptions } from '../rosetta-translator';
import { LanguageTablet } from '../tablets/tablets';
import { RosettaDiagnostic } from '../translate';
export interface ExtractResult {
    diagnostics: RosettaDiagnostic[];
    tablet: LanguageTablet;
}
export interface ExtractOptions {
    readonly includeCompilerDiagnostics?: boolean;
    readonly validateAssemblies?: boolean;
    readonly only?: string[];
    /**
     * A tablet file to be loaded and used as a source for caching
     */
    readonly cacheFromFile?: string;
    /**
     * A tablet file to append translated snippets to
     */
    readonly cacheToFile?: string;
    /**
     * Trim cache to only contain translations found in the current assemblies
     *
     * @default false
     */
    readonly trimCache?: boolean;
    /**
     * Write translations to implicit tablets (`.jsii.tabl.json`)
     *
     * @default true
     */
    readonly writeToImplicitTablets?: boolean;
    /**
     * What directory to compile the samples in
     *
     * @default - Rosetta manages the compilation directory
     * @deprecated Samples declare their own dependencies instead
     */
    readonly compilationDirectory?: string;
    /**
     * Make a translator (just for testing)
     */
    readonly translatorFactory?: (opts: RosettaTranslatorOptions) => RosettaTranslator;
    /**
     * Turn on 'loose mode' or not
     *
     * Loose mode ignores failures during fixturizing, and undoes 'strict mode' for
     * diagnostics.
     *
     * @default false
     */
    readonly loose?: boolean;
    /**
     * Accept dirty translations from the cache
     *
     * @default false
     */
    readonly allowDirtyTranslations?: boolean;
    /**
     * Compress the implicit tablet files.
     *
     * @default - preserves the original compression status of each individual implicit tablet file.
     */
    readonly compressTablet?: boolean;
    /**
     * Compress the cacheToFile tablet.
     *
     * @default false
     */
    readonly compressCacheToFile?: boolean;
    /**
     * Cleanup temporary directories
     *
     * @default true
     */
    readonly cleanup?: boolean;
}
export declare function extractAndInfuse(assemblyLocations: string[], options: ExtractOptions): Promise<ExtractResult>;
/**
 * Extract all samples from the given assemblies into a tablet
 */
export declare function extractSnippets(assemblyLocations: readonly string[], options?: ExtractOptions): Promise<ExtractResult>;
//# sourceMappingURL=extract.d.ts.map