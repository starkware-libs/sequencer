import { Assembly } from './assembly';
/**
 * Describes a source location in a file
 */
export interface SourceLocation {
    /**
     * The file name
     */
    filename: string;
    /**
     * The 1-based line inside the file
     */
    line: number;
}
/**
 * Interface for API items that can be queried for a source location
 */
export interface SourceLocatable {
    /**
     * The assembly the API item is defined in
     */
    readonly assembly: Assembly;
    /**
     * Source location relative to the assembly root
     */
    readonly locationInModule?: SourceLocation;
}
/**
 * Return the repository location for the given API item
 */
export declare function locationInRepository(item: SourceLocatable): SourceLocation | undefined;
/**
 * Return a URL for this item into the source repository, if available
 *
 * (Currently only supports GitHub URLs)
 */
export declare function repositoryUrl(item: SourceLocatable, ref?: string): string | undefined;
//# sourceMappingURL=source.d.ts.map