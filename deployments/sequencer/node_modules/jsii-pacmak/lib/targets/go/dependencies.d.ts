import { Package } from './package';
/**
 * Information about a module's dependency on "special" packages (either part of
 * the go standard library, or generated as part of the current module).
 */
export interface SpecialDependencies {
    /** Whether the go standard library for string formatting is needed */
    readonly fmt: boolean;
    /** Whether the jsii runtime library for go is needed */
    readonly runtime: boolean;
    /** Whether the package's initialization hook is needed */
    readonly init: boolean;
    /** Whether the internal type aliases package is needed */
    readonly internal: boolean;
    /** Whether go's standard library "time" module is needed */
    readonly time: boolean;
}
export declare function reduceSpecialDependencies(...specialDepsList: readonly SpecialDependencies[]): SpecialDependencies;
export interface ImportedModule {
    readonly alias?: string;
    readonly module: string;
}
export declare function toImportedModules(specialDeps: SpecialDependencies, context: Package): readonly ImportedModule[];
/**
 * The name of a sub-package that includes internal type aliases it has to be
 * "internal" so it not published.
 */
export declare const INTERNAL_PACKAGE_NAME = "internal";
export declare const JSII_RT_MODULE: ImportedModule;
export declare const GO_REFLECT: ImportedModule;
//# sourceMappingURL=dependencies.d.ts.map