export interface TrimCacheOptions {
    /**
     * Locations of assemblies to search for snippets
     */
    readonly assemblyLocations: string[];
    /**
     * Cache to trim
     */
    readonly cacheFile: string;
}
export declare function trimCache(options: TrimCacheOptions): Promise<void>;
//# sourceMappingURL=trim-cache.d.ts.map