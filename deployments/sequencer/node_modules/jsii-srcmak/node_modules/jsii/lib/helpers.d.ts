/**
 * Helper routines for use with the jsii compiler
 *
 * These are mostly used for testing, but all projects that need to exercise
 * the JSII compiler to test something need to share this code, so might as
 * well put it in one reusable place.
 */
import { PackageJson } from '@jsii/spec';
import * as spec from '@jsii/spec';
import { CompilerOptions } from './compiler';
/**
 * A set of source files for `sourceToAssemblyHelper`, at least containing 'index.ts'
 */
export type MultipleSourceFiles = {
    'index.ts': string;
    [name: string]: string;
};
/**
 * Compile a piece of source and return the JSII assembly for it
 *
 * Only usable for trivial cases and tests.
 *
 * @param source can either be a single `string` (the content of `index.ts`), or
 *               a map of fileName to content, which *must* include `index.ts`.
 * @param options accepts a callback for historical reasons but really expects to
 *                take an options object.
 */
export declare function sourceToAssemblyHelper(source: string | MultipleSourceFiles, options?: TestCompilationOptions | ((obj: PackageJson) => void)): spec.Assembly;
export interface HelperCompilationResult {
    /**
     * The generated assembly
     */
    readonly assembly: spec.Assembly;
    /**
     * Generated .js/.d.ts file(s)
     */
    readonly files: Record<string, string>;
    /**
     * The packageInfo used
     */
    readonly packageJson: PackageJson;
    /**
     * Whether to compress the assembly file
     */
    readonly compressAssembly: boolean;
}
/**
 * Compile a piece of source and return the assembly and compiled sources for it
 *
 * Only usable for trivial cases and tests.
 *
 * @param source can either be a single `string` (the content of `index.ts`), or
 *               a map of fileName to content, which *must* include `index.ts`.
 * @param options accepts a callback for historical reasons but really expects to
 *                take an options object.
 */
export declare function compileJsiiForTest(source: string | {
    'index.ts': string;
    [name: string]: string;
}, options?: TestCompilationOptions | ((obj: PackageJson) => void), compilerOptions?: Omit<CompilerOptions, 'projectInfo' | 'watch'>): HelperCompilationResult;
export interface TestCompilationOptions {
    /**
     * The directory in which we write and compile the files
     */
    readonly compilationDirectory?: string;
    /**
     * Parts of projectInfo to override (package name etc)
     *
     * @deprecated Prefer using `packageJson` instead.
     */
    readonly projectInfo?: Partial<PackageJson>;
    /**
     * Parts of projectInfo to override (package name etc)
     *
     * @default - Use some default values
     */
    readonly packageJson?: Partial<PackageJson>;
    /**
     * Whether to compress the assembly file.
     *
     * @default false
     */
    readonly compressAssembly?: boolean;
}
/**
 * An NPM-ready workspace where we can install test-compile dependencies and compile new assemblies
 */
export declare class TestWorkspace {
    readonly rootDirectory: string;
    /**
     * Create a new workspace.
     *
     * Creates a temporary directory, don't forget to call cleanUp
     */
    static create(): TestWorkspace;
    /**
     * Execute a block with a temporary workspace
     */
    static withWorkspace<A>(block: (ws: TestWorkspace) => A): A;
    private readonly installed;
    private constructor();
    /**
     * Add a test-compiled jsii assembly as a dependency
     */
    addDependency(dependencyAssembly: HelperCompilationResult): void;
    dependencyDir(name: string): string;
    cleanup(): void;
}
export type PackageInfo = PackageJson;
/**
 * TSConfig paths can either be relative to the project or absolute.
 * This function normalizes paths to be relative to the provided root.
 * After normalization, code using these paths can be much simpler.
 *
 * @param root the project root
 * @param pathToNormalize the path to normalize, might be empty
 */
export declare function normalizeConfigPath(root: string, pathToNormalize?: string): string | undefined;
//# sourceMappingURL=helpers.d.ts.map