import * as spec from '@jsii/spec';
import { TypeScriptSnippet, ApiLocation } from '../snippet';
import { LanguageTablet } from '../tablets/tablets';
/**
 * The JSDoc tag users can use to associate non-visible metadata with an example
 *
 * In a Markdown section, metadata goes after the code block fence, where it will
 * be attached to the example but invisible.
 *
 *    ```ts metadata=goes here
 *
 * But in doc comments, '@example' already delineates the example, and any metadata
 * in there added by the '///' tags becomes part of the visible code (there is no
 * place to put hidden information).
 *
 * We introduce the '@exampleMetadata' tag to put that additional information.
 */
export declare const EXAMPLE_METADATA_JSDOCTAG = "exampleMetadata";
interface RosettaPackageJson extends spec.PackageJson {
    readonly jsiiRosetta?: {
        readonly strict?: boolean;
        readonly exampleDependencies?: Record<string, string>;
    };
}
export interface LoadedAssembly {
    readonly assembly: spec.Assembly;
    readonly directory: string;
    readonly packageJson?: RosettaPackageJson;
}
/**
 * Load assemblies by filename or directory
 */
export declare function loadAssemblies(assemblyLocations: readonly string[], validateAssemblies: boolean): readonly LoadedAssembly[];
/**
 * Load the default tablets for every assembly, if available
 *
 * Returns a map of { directory -> tablet }.
 */
export declare function loadAllDefaultTablets(asms: readonly LoadedAssembly[]): Promise<Record<string, LanguageTablet>>;
/**
 * Returns the location of the tablet file, either .jsii.tabl.json or .jsii.tabl.json.gz.
 * Assumes that a tablet exists in the directory and if not, the ensuing behavior is
 * handled by the caller of this function.
 */
export declare function guessTabletLocation(directory: string): string;
export declare function compressedTabletExists(directory: string): boolean;
export type AssemblySnippetSource = {
    type: 'markdown';
    markdown: string;
    location: ApiLocation;
} | {
    type: 'example';
    source: string;
    metadata?: {
        [key: string]: string;
    };
    location: ApiLocation;
};
/**
 * Return all markdown and example snippets from the given assembly
 */
export declare function allSnippetSources(assembly: spec.Assembly): AssemblySnippetSource[];
export declare function allTypeScriptSnippets(assemblies: readonly LoadedAssembly[], loose?: boolean): Promise<TypeScriptSnippet[]>;
export interface TypeLookupAssembly {
    readonly packageJson: any;
    readonly assembly: spec.Assembly;
    readonly directory: string;
    readonly symbolIdMap: Record<string, string>;
}
/**
 * Recursively searches for a .jsii file in the directory.
 * When file is found, checks cache to see if we already
 * stored the assembly in memory. If not, we synchronously
 * load the assembly into memory.
 */
export declare function findTypeLookupAssembly(startingDirectory: string): TypeLookupAssembly | undefined;
/**
 * Find the jsii [sub]module that contains the given FQN
 *
 * @returns `undefined` if the type is a member of the assembly root.
 */
export declare function findContainingSubmodule(assembly: spec.Assembly, fqn: string): string | undefined;
export {};
//# sourceMappingURL=assemblies.d.ts.map