import type { Assembly, TypeSystem } from 'jsii-reflect';
export declare const DEFAULT_PACK_COMMAND = "npm pack";
export interface JsiiModuleOptions {
    /**
     * Name of the module
     */
    name: string;
    /**
     * The module directory
     */
    moduleDirectory: string;
    /**
     * Identifier of the targets to build
     */
    availableTargets: string[];
    /**
     * Output directory where to package everything
     */
    defaultOutputDirectory: string;
    /**
     * Names of packages this package depends on, if any
     */
    dependencyNames?: string[];
}
export declare class JsiiModule {
    readonly name: string;
    readonly dependencyNames: string[];
    readonly moduleDirectory: string;
    readonly availableTargets: string[];
    outputDirectory: string;
    private _tarball?;
    _assembly?: Assembly;
    constructor(options: JsiiModuleOptions);
    /**
     * Prepare an NPM package from this source module
     */
    npmPack(packCommand?: string): Promise<void>;
    get tarball(): string;
    load(system: TypeSystem, validate?: boolean): Promise<Assembly>;
    get assembly(): Assembly;
    cleanup(): Promise<void>;
}
//# sourceMappingURL=packaging.d.ts.map