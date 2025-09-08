import * as ts from 'typescript';
import { Emitter } from './emitter';
import { ProjectInfo } from './project-info';
import { TypeScriptConfigValidationRuleSet } from './tsconfig';
export declare const DIAGNOSTICS = "diagnostics";
export declare const JSII_DIAGNOSTICS_CODE = 9999;
export interface CompilerOptions {
    /** The information about the project to be built */
    projectInfo: ProjectInfo;
    /** Whether the compiler should watch for changes or just compile once */
    watch?: boolean;
    /** Whether to detect and generate TypeScript project references */
    projectReferences?: boolean;
    /** Whether to fail when a warning is emitted */
    failOnWarnings?: boolean;
    /** Whether to strip deprecated members from emitted artifacts */
    stripDeprecated?: boolean;
    /** The path to an allowlist of FQNs to strip if stripDeprecated is set */
    stripDeprecatedAllowListFile?: string;
    /** Whether to add warnings for deprecated elements */
    addDeprecationWarnings?: boolean;
    /**
     * The name of the tsconfig file to generate.
     * Cannot be used at the same time as `typeScriptConfig`.
     * @default "tsconfig.json"
     */
    generateTypeScriptConfig?: string;
    /**
     * The name of the tsconfig file to use.
     * Cannot be used at the same time as `generateTypeScriptConfig`.
     * @default - generate the tsconfig file
     */
    typeScriptConfig?: string;
    /**
     * The ruleset to validate the provided tsconfig file against.
     * Can only be used when `typeScriptConfig` is provided.
     * @default TypeScriptConfigValidationRuleSet.STRICT - if `typeScriptConfig` is provided
     */
    validateTypeScriptConfig?: TypeScriptConfigValidationRuleSet;
    /**
     * Whether to compress the assembly
     * @default false
     */
    compressAssembly?: boolean;
}
export declare class Compiler implements Emitter {
    private readonly options;
    private readonly system;
    private readonly compilerHost;
    private readonly userProvidedTypeScriptConfig;
    private readonly tsconfig;
    private rootFiles;
    private readonly configPath;
    private readonly projectRoot;
    constructor(options: CompilerOptions);
    /**
     * Compiles the configured program.
     *
     * @param files can be specified to override the standard source code location logic. Useful for example when testing "negatives".
     */
    emit(...files: string[]): ts.EmitResult;
    /**
     * Watches for file-system changes and dynamically recompiles the project as needed. In blocking mode, this results
     * in a never-resolving promise.
     */
    watch(): Promise<never>;
    /**
     * Prepares the project for build, by creating the necessary configuration
     * file(s), and assigning the relevant root file(s).
     *
     * @param files the files that were specified as input in the CLI invocation.
     */
    private configureTypeScript;
    /**
     * Final preparations of the project for build.
     *
     * These are preparations that either
     * - must happen immediately before the build, or
     * - can be different for every build like assigning the relevant root file(s).
     *
     * @param files the files that were specified as input in the CLI invocation.
     */
    private prepareForBuild;
    /**
     * Do a single build
     */
    private buildOnce;
    private consumeProgram;
    /**
     * Build the TypeScript config object from jsii config
     *
     * This is the object that will be written to disk
     * unless an existing tsconfig was provided.
     */
    private buildTypeScriptConfig;
    /**
     * Load the TypeScript config object from a provided file
     */
    private readTypeScriptConfig;
    /**
     * Creates a `tsconfig.json` file to improve the IDE experience.
     *
     * @return the fully qualified path to the `tsconfig.json` file
     */
    private writeTypeScriptConfig;
    /**
     * Find all dependencies that look like TypeScript projects.
     *
     * Enumerate all dependencies, if they have a tsconfig.json file with
     * "composite: true" we consider them project references.
     *
     * (Note: TypeScript seems to only correctly find transitive project references
     * if there's an "index" tsconfig.json of all projects somewhere up the directory
     * tree)
     */
    private findProjectReferences;
    /**
     * Find source files using the same mechanism that the TypeScript compiler itself uses.
     *
     * Respects includes/excludes/etc.
     *
     * This makes it so that running 'typescript' and running 'jsii' has the same behavior.
     */
    private determineSources;
    /**
     * Resolve the given dependency name from the current package, and find the associated tsconfig.json location
     *
     * Because we have the following potential directory layout:
     *
     *   package/node_modules/some_dependency
     *   package/tsconfig.json
     *
     * We resolve symlinks and only find a "TypeScript" dependency if doesn't have 'node_modules' in
     * the path after resolving symlinks (i.e., if it's a peer package in the same monorepo).
     *
     * Returns undefined if no such tsconfig could be found.
     */
    private findMonorepoPeerTsconfig;
    private diagsHaveAbortableErrors;
}
//# sourceMappingURL=compiler.d.ts.map