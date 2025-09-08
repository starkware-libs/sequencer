import { Assembly } from './assembly';
import { ClassType } from './class';
import { EnumType } from './enum';
import { InterfaceType } from './interface';
import { Method } from './method';
import { Property } from './property';
import { Type } from './type';
export declare class TypeSystem {
    /**
     * The "root" assemblies (ones that loaded explicitly via a "load" call).
     */
    readonly roots: Assembly[];
    private readonly _assemblyLookup;
    private readonly _cachedClasses;
    private _locked;
    get isLocked(): boolean;
    /**
     * All assemblies in this type system.
     */
    get assemblies(): readonly Assembly[];
    /**
     * Locks the TypeSystem from further changes
     *
     * Call this once all assemblies have been loaded.
     * This allows the reflection to optimize and cache certain expensive calls.
     */
    lock(): void;
    /**
     * Load all JSII dependencies of the given NPM package directory.
     *
     * The NPM package itself does *not* have to be a jsii package, and does
     * NOT have to declare a JSII dependency on any of the packages.
     */
    loadNpmDependencies(packageRoot: string, options?: {
        validate?: boolean;
    }): Promise<void>;
    /**
     * Loads a jsii module or a single .jsii file into the type system.
     *
     * If `fileOrDirectory` is a directory, it will be treated as a jsii npm module,
     * and its dependencies (as determined by its 'package.json' file) will be loaded
     * as well.
     *
     * If `fileOrDirectory` is a file, it will be treated as a single .jsii file.
     * No dependencies will be loaded. You almost never want this.
     *
     * Not validating makes the difference between loading assemblies with lots
     * of dependencies (such as app-delivery) in 90ms vs 3500ms.
     *
     * @param fileOrDirectory A .jsii file path or a module directory
     * @param validate Whether or not to validate the assembly while loading it.
     */
    load(fileOrDirectory: string, options?: {
        validate?: boolean;
    }): Promise<Assembly>;
    loadModule(dir: string, options?: {
        validate?: boolean;
    }): Promise<Assembly>;
    loadFile(file: string, options?: {
        isRoot?: boolean;
        validate?: boolean;
    }): Assembly;
    addAssembly(asm: Assembly, options?: {
        isRoot?: boolean;
    }): Assembly;
    /**
     * Determines whether this TypeSystem includes a given assembly.
     *
     * @param name the name of the assembly being looked for.
     */
    includesAssembly(name: string): boolean;
    isRoot(name: string): boolean;
    findAssembly(name: string): Assembly;
    tryFindAssembly(name: string): Assembly | undefined;
    findFqn(fqn: string): Type;
    tryFindFqn(fqn: string): Type | undefined;
    findClass(fqn: string): ClassType;
    findInterface(fqn: string): InterfaceType;
    findEnum(fqn: string): EnumType;
    /**
     * All methods in the type system.
     */
    get methods(): readonly Method[];
    /**
     * All properties in the type system.
     */
    get properties(): readonly Property[];
    /**
     * All classes in the type system.
     */
    get classes(): readonly ClassType[];
    /**
     * All interfaces in the type system.
     */
    get interfaces(): readonly InterfaceType[];
    /**
     * All enums in the type system.
     */
    get enums(): readonly EnumType[];
    /**
     * Load an assembly without adding it to the typesystem
     * @param file Assembly file to load
     * @param validate Whether to validate the assembly or just assume it matches the schema
     */
    private loadAssembly;
    private addRoot;
}
//# sourceMappingURL=type-system.d.ts.map