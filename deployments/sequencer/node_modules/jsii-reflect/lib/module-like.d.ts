import * as jsii from '@jsii/spec';
import { ClassType } from './class';
import { EnumType } from './enum';
import { InterfaceType } from './interface';
import { Submodule } from './submodule';
import { Type } from './type';
import { TypeSystem } from './type-system';
export declare abstract class ModuleLike {
    readonly system: TypeSystem;
    abstract readonly fqn: string;
    /**
     * A map of target name to configuration, which is used when generating packages for
     * various languages.
     */
    abstract readonly targets?: jsii.AssemblyTargets;
    abstract readonly readme?: jsii.ReadMe;
    protected abstract readonly submoduleMap: ReadonlyMap<string, Submodule>;
    protected abstract readonly typeMap: ReadonlyMap<string, Type>;
    /**
     * Cache for the results of `tryFindType`.
     */
    private readonly typeLocatorCache;
    protected constructor(system: TypeSystem);
    get submodules(): readonly Submodule[];
    /**
     * All types in this module/namespace (not submodules)
     */
    get types(): readonly Type[];
    /**
     * All classes in this module/namespace (not submodules)
     */
    get classes(): readonly ClassType[];
    /**
     * All interfaces in this module/namespace (not submodules)
     */
    get interfaces(): readonly InterfaceType[];
    /**
     * All enums in this module/namespace (not submodules)
     */
    get enums(): readonly EnumType[];
    tryFindType(fqn: string): Type | undefined;
}
//# sourceMappingURL=module-like.d.ts.map