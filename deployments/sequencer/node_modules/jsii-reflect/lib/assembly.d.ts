import * as jsii from '@jsii/spec';
import { ClassType } from './class';
import { Dependency } from './dependency';
import { EnumType } from './enum';
import { InterfaceType } from './interface';
import { ModuleLike } from './module-like';
import { Submodule } from './submodule';
import { Type } from './type';
import { TypeSystem } from './type-system';
export declare class Assembly extends ModuleLike {
    readonly spec: jsii.Assembly;
    private _typeCache?;
    private _submoduleCache?;
    private _dependencyCache?;
    constructor(system: TypeSystem, spec: jsii.Assembly);
    get fqn(): string;
    /**
     * The version of the spec schema
     */
    get schema(): jsii.SchemaVersion;
    /**
     * The version of the jsii compiler that was used to produce this Assembly.
     */
    get jsiiVersion(): string;
    /**
     * The name of the assembly
     */
    get name(): string;
    /**
     * Description of the assembly, maps to "description" from package.json
     * This is required since some package managers (like Maven) require it.
     */
    get description(): string;
    /**
     * The metadata associated with the assembly, if any.
     */
    get metadata(): {
        readonly [key: string]: any;
    } | undefined;
    /**
     * The url to the project homepage. Maps to "homepage" from package.json.
     */
    get homepage(): string;
    /**
     * The module repository, maps to "repository" from package.json
     * This is required since some package managers (like Maven) require it.
     */
    get repository(): {
        readonly type: string;
        readonly url: string;
        readonly directory?: string;
    };
    /**
     * The main author of this package.
     */
    get author(): jsii.Person;
    /**
     * Additional contributors to this package.
     */
    get contributors(): readonly jsii.Person[];
    /**
     * A fingerprint that can be used to determine if the specification has changed.
     */
    get fingerprint(): string;
    /**
     * The version of the assembly
     */
    get version(): string;
    /**
     * The SPDX name of the license this assembly is distributed on.
     */
    get license(): string;
    /**
     * A map of target name to configuration, which is used when generating packages for
     * various languages.
     */
    get targets(): jsii.AssemblyTargets | undefined;
    /**
     * Dependencies on other assemblies (with semver), the key is the JSII assembly name.
     */
    get dependencies(): readonly Dependency[];
    findDependency(name: string): Dependency;
    /**
     * List if bundled dependencies (these are not expected to be jsii assemblies).
     */
    get bundled(): {
        readonly [module: string]: string;
    };
    /**
     * The top-level readme document for this assembly (if any).
     */
    get readme(): jsii.ReadMe | undefined;
    /**
     * Return the those submodules nested directly under the assembly
     */
    get submodules(): readonly Submodule[];
    /**
     * Return all submodules, even those transtively nested
     */
    get allSubmodules(): readonly Submodule[];
    /**
     * All types in the assembly and all of its submodules
     */
    get allTypes(): readonly Type[];
    /**
     * All classes in the assembly and all of its submodules
     */
    get allClasses(): readonly ClassType[];
    /**
     * All interfaces in the assembly and all of its submodules
     */
    get allInterfaces(): readonly InterfaceType[];
    /**
     * All interfaces in the assembly and all of its submodules
     */
    get allEnums(): readonly EnumType[];
    findType(fqn: string): Type;
    /**
     * Validate an assembly after loading
     *
     * If the assembly was loaded without validation, call this to validate
     * it after all. Throws an exception if validation fails.
     */
    validate(): void;
    protected get submoduleMap(): ReadonlyMap<string, Submodule>;
    /**
     * All types in the root of the assembly
     */
    protected get typeMap(): ReadonlyMap<string, Type>;
    private get _dependencies();
    private _analyzeTypes;
    /**
     * Return a builder for all submodules in this assembly (so that we can
     * add types into the objects).
     */
    private discoverSubmodules;
}
//# sourceMappingURL=assembly.d.ts.map