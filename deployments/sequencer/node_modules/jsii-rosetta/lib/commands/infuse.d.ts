export interface InfuseResult {
    readonly coverageResults: Record<string, InfuseTypes>;
}
export interface InfuseTypes {
    readonly types: number;
    readonly typesWithInsertedExamples: number;
}
export interface InfuseOptions {
    readonly logFile?: string;
    /**
     * Where to read additional translations
     */
    readonly cacheFromFile?: string;
    /**
     * In addition to the implicit tablets, also write all added examples to this additional output tablet
     */
    readonly cacheToFile?: string;
    /**
     * Compress the cacheToFile
     */
    readonly compressCacheToFile?: boolean;
}
export declare const DEFAULT_INFUSION_RESULTS_NAME = "infusion-results.html";
/**
 * Infuse will analyze the snippets in a set of tablets, and update the assembly to add
 * examples to types that don't have any yet, based on snippets that use the given type.
 */
export declare function infuse(assemblyLocations: string[], options?: InfuseOptions): Promise<InfuseResult>;
//# sourceMappingURL=infuse.d.ts.map