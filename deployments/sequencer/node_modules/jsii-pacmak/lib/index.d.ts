import { UnknownSnippetMode } from 'jsii-rosetta';
import { TargetName } from './targets';
import { Timers } from './timer';
export { TargetName };
export { configure as configureLogging } from './logging';
/**
 * Generates code in the desired targets.
 */
export declare function pacmak({ argv, clean, codeOnly, fingerprint, force, forceSubdirectory, forceTarget, inputDirectories, outputDirectory, parallel, recurse, rosettaTablet, rosettaUnknownSnippets, runtimeTypeChecking, targets, timers, updateNpmIgnoreFiles, validateAssemblies, }: PacmakOptions): Promise<void>;
/**
 * Options provided to the `pacmak` function.
 */
export interface PacmakOptions {
    /**
     * All command-line arguments that were provided. This includes target-specific parameters, the
     * handling of which is up to the code generators.
     *
     * @default {}
     */
    readonly argv?: {
        readonly [name: string]: any;
    };
    /**
     * Whether to clean up temporary directories upon completion.
     *
     * @default true
     */
    readonly clean?: boolean;
    /**
     * Whether to generate source code only (as opposed to built packages).
     *
     * @default false
     */
    readonly codeOnly?: boolean;
    /**
     * Whether to opportunistically include a fingerprint in generated code, to avoid re-generating
     * code if the source assembly has not changed.
     *
     * @default true
     */
    readonly fingerprint?: boolean;
    /**
     * Whether to always re-generate code, even if the fingerprint has not changed.
     *
     * @default false
     */
    readonly force?: boolean;
    /**
     * Always emit code in a per-language subdirectory, even if there is only one target language.
     *
     * @default true
     */
    readonly forceSubdirectory?: boolean;
    /**
     * Always try to generate code for the selected targets, even if those are not configured. Use this option at your own
     * risk, as there are significant chances code generators cannot operate without any configuration.
     *
     * @default false
     */
    readonly forceTarget?: boolean;
    /**
     * The list of directories to be considered for input assemblies.
     */
    readonly inputDirectories: readonly string[];
    /**
     * The directory in which to output generated packages or code (if  `codeOnly` is `true`).
     *
     * @default - Configured in `package.json`
     */
    readonly outputDirectory?: string;
    /**
     * Whether to parallelize code generation. Turning this to `false` can be beneficial in certain resource-constrained
     * environments, such as free CI/CD offerings, as it reduces the pressure on IO.
     *
     * @default true
     */
    readonly parallel?: boolean;
    /**
     * Whether to recursively generate for the selected packages' dependencies.
     *
     * @default false
     */
    readonly recurse?: boolean;
    /**
     * How rosetta should treat snippets that cannot be loaded from a translation tablet.
     *
     * @default UnknownSnippetMode.VERBATIM
     */
    readonly rosettaUnknownSnippets?: UnknownSnippetMode;
    /**
     * A Rosetta tablet file where translations for code examples can be found.
     *
     * @default undefined
     */
    readonly rosettaTablet?: string;
    /**
     * Whether to inject runtime type checks in places where compile-time type checking is not performed.
     *
     * @default true
     */
    readonly runtimeTypeChecking?: boolean;
    /**
     * The list of targets for which code should be generated. Unless `forceTarget` is `true`, a given target will only
     * be generated for assemblies that have configured it.
     *
     * @default Object.values(TargetName)
     */
    readonly targets?: readonly TargetName[];
    /**
     * A `Timers` object, if you are interested in including the rosetta run in a larger set of timed operations.
     */
    readonly timers?: Timers;
    /**
     * Whether to update .npmignore files if `outputDirectory` comes from the `package.json` files.
     *
     * @default false
     */
    readonly updateNpmIgnoreFiles?: boolean;
    /**
     * Whether assemblies should be validated or not. Validation can be expensive and can be skipped if the assemblies
     * can be assumed to be valid.
     *
     * @default false
     */
    readonly validateAssemblies?: boolean;
}
//# sourceMappingURL=index.d.ts.map