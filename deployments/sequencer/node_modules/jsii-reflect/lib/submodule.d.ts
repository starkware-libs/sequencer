import * as jsii from '@jsii/spec';
import { ModuleLike } from './module-like';
import { Type } from './type';
import { TypeSystem } from './type-system';
export declare class Submodule extends ModuleLike {
    readonly spec: jsii.Submodule;
    readonly fqn: string;
    protected readonly submoduleMap: ReadonlyMap<string, Submodule>;
    protected readonly typeMap: ReadonlyMap<string, Type>;
    /**
     * The simple name of the submodule (the last segment of the `fullName`).
     */
    readonly name: string;
    constructor(system: TypeSystem, spec: jsii.Submodule, fqn: string, submoduleMap: ReadonlyMap<string, Submodule>, typeMap: ReadonlyMap<string, Type>);
    /**
     * A map of target name to configuration, which is used when generating packages for
     * various languages.
     */
    get targets(): jsii.AssemblyTargets | undefined;
    /**
     * The top-level readme document for this assembly (if any).
     */
    get readme(): jsii.ReadMe | undefined;
}
//# sourceMappingURL=submodule.d.ts.map