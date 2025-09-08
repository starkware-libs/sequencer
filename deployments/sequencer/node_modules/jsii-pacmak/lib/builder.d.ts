import { RosettaTabletReader } from 'jsii-rosetta';
import { JsiiModule } from './packaging';
import { TargetConstructor } from './target';
import { TargetName } from './targets';
import { Toposorted } from './toposort';
export interface BuildOptions {
    /**
     * Whether to fingerprint the produced artifacts.
     * @default true
     */
    readonly fingerprint?: boolean;
    /**
     * Whether artifacts should be re-build even if their fingerprints look up-to-date.
     * @default false
     */
    readonly force?: boolean;
    /**
     * Arguments provided by the user (how they are used is target-dependent)
     */
    readonly arguments: {
        readonly [name: string]: any;
    };
    /**
     * Only generate code, don't build
     */
    readonly codeOnly?: boolean;
    /**
     * Whether or not to clean
     */
    readonly clean?: boolean;
    /**
     * Whether to add an additional subdirectory for the target language
     */
    readonly languageSubdirectory?: boolean;
    /**
     * The Rosetta instance to load examples from
     */
    readonly rosetta: RosettaTabletReader;
    /**
     * Whether to generate runtime type checking code in places where compile-time
     * type checking is not possible.
     */
    readonly runtimeTypeChecking: boolean;
}
/**
 * Interface for classes that can build language targets
 *
 * Building can happen one target at a time, or multiple targets at a time.
 */
export interface TargetBuilder {
    buildModules(): Promise<void>;
}
/**
 * Base implementation, building the package targets for the given language independently of each other
 *
 * Some languages can gain substantial speedup in preparing an "uber project" for all packages
 * and compiling them all in one go (Those will be implementing a custom Builder).
 *
 * For languages where it doesn't matter--or where we haven't figured out how to
 * do that yet--this class can serve as a base class: it will build each package
 * independently, taking care to build them in the right order.
 */
export declare class IndependentPackageBuilder implements TargetBuilder {
    private readonly targetName;
    private readonly targetConstructor;
    private readonly modules;
    private readonly options;
    constructor(targetName: TargetName, targetConstructor: TargetConstructor, modules: Toposorted<JsiiModule>, options: BuildOptions);
    buildModules(): Promise<void>;
    private generateModuleCode;
    private buildModule;
    private makeTarget;
    private finalOutputDir;
}
//# sourceMappingURL=builder.d.ts.map