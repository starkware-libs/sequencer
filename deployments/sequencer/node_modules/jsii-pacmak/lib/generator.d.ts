import * as spec from '@jsii/spec';
import { CodeMaker } from 'codemaker';
import * as reflect from 'jsii-reflect';
/**
 * Options for the code generator framework.
 */
export interface GeneratorOptions {
    /**
     * If this property is set to 'true', union properties are "expanded" into multiple
     * properties, each with a different type and a postfix based on the type name. This
     * can be used by languages that don't have support for union types (e.g. Java).
     */
    expandUnionProperties?: boolean;
    /**
     * If this property is set to 'true', methods that have optional arguments are duplicated
     * and overloads are created with all parameters.
     */
    generateOverloadsForMethodWithOptionals?: boolean;
    /**
     * If this property is set, the generator will add "Base" to abstract class names
     */
    addBasePostfixToAbstractClassNames?: boolean;
    /**
     * If this property is set, the generator will add runtime type checking code in places
     * where compile-time type checking is not possible.
     */
    runtimeTypeChecking: boolean;
}
export interface IGenerator {
    /**
     *
     * @param fingerprint
     */
    generate(fingerprint: boolean): void;
    /**
     * Load a module into the generator.
     * @param packageDir is the root directory of the module.
     */
    load(packageDir: string, assembly: reflect.Assembly): Promise<void>;
    /**
     * Determine if the generated artifacts for this generator are already up-to-date.
     *
     * @param outDir the directory where generated artifacts would be placed.
     * @param tarball the tarball of the bundled node library
     * @param legalese the license and notice file contents (if any)
     *
     * @return ``true`` if no generation is necessary
     */
    upToDate(outDir: string): Promise<boolean>;
    /**
     * Saves the generated code in the provided output directory.
     *
     * @param outdir the directory in which to place generated code.
     * @param tarball the bundled npm library backing the generated code.
     * @param legalese the LICENSE & NOTICE contents for this package.
     */
    save(outdir: string, tarball: string, legalese: Legalese): Promise<any>;
}
export interface Legalese {
    /**
     * The text of the SPDX license associated with this package, if any.
     */
    readonly license?: string;
    /**
     * The contents of the NOTICE file for this package, if any.
     */
    readonly notice?: string;
}
/**
 * Abstract base class for jsii package generators.
 * Given a jsii module, it will invoke "events" to emit various elements.
 */
export declare abstract class Generator implements IGenerator {
    private readonly options;
    private readonly excludeTypes;
    protected readonly code: CodeMaker;
    private _assembly?;
    protected _reflectAssembly?: reflect.Assembly;
    private fingerprint?;
    constructor(options: GeneratorOptions);
    protected get runtimeTypeChecking(): boolean;
    protected get assembly(): spec.Assembly;
    get reflectAssembly(): reflect.Assembly;
    get metadata(): {
        fingerprint: string | undefined;
    };
    load(_packageRoot: string, assembly: reflect.Assembly): Promise<void>;
    /**
     * Runs the generator (in-memory).
     */
    generate(fingerprint: boolean): void;
    upToDate(_: string): Promise<boolean>;
    /**
     * Returns the file name of the assembly resource as it is going to be saved.
     */
    protected getAssemblyFileName(): string;
    /**
     * Saves all generated files to an output directory, creating any subdirs if needed.
     */
    save(outdir: string, tarball: string, { license, notice }: Legalese): Promise<string[]>;
    /**
     * Returns the destination directory for the assembly file.
     */
    protected getAssemblyOutputDir(_mod: spec.Assembly): string | undefined;
    protected onBeginAssembly(_assm: spec.Assembly, _fingerprint: boolean): void;
    protected onEndAssembly(_assm: spec.Assembly, _fingerprint: boolean): void;
    protected onBeginNamespace(_ns: string): void;
    protected onEndNamespace(_ns: string): void;
    protected onBeginClass(_cls: spec.ClassType, _abstract: boolean | undefined): void;
    protected onEndClass(_cls: spec.ClassType): void;
    protected abstract onBeginInterface(ifc: spec.InterfaceType): void;
    protected abstract onEndInterface(ifc: spec.InterfaceType): void;
    protected abstract onInterfaceMethod(ifc: spec.InterfaceType, method: spec.Method): void;
    protected abstract onInterfaceMethodOverload(ifc: spec.InterfaceType, overload: spec.Method, originalMethod: spec.Method): void;
    protected abstract onInterfaceProperty(ifc: spec.InterfaceType, prop: spec.Property): void;
    protected onInitializer(_cls: spec.ClassType, _initializer: spec.Initializer): void;
    protected onInitializerOverload(_cls: spec.ClassType, _overload: spec.Initializer, _originalInitializer: spec.Initializer): void;
    protected onBeginProperties(_cls: spec.ClassType): void;
    protected abstract onProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected abstract onStaticProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onEndProperties(_cls: spec.ClassType): void;
    protected abstract onUnionProperty(cls: spec.ClassType, prop: spec.Property, union: spec.UnionTypeReference): void;
    protected onExpandedUnionProperty(_cls: spec.ClassType, _prop: spec.Property, _primaryName: string): void;
    protected onBeginMethods(_cls: spec.ClassType): void;
    protected abstract onMethod(cls: spec.ClassType, method: spec.Method): void;
    protected abstract onMethodOverload(cls: spec.ClassType, overload: spec.Method, originalMethod: spec.Method): void;
    protected abstract onStaticMethod(cls: spec.ClassType, method: spec.Method): void;
    protected abstract onStaticMethodOverload(cls: spec.ClassType, overload: spec.Method, originalMethod: spec.Method): void;
    protected onEndMethods(_cls: spec.ClassType): void;
    protected onBeginEnum(_enm: spec.EnumType): void;
    protected onEndEnum(_enm: spec.EnumType): void;
    protected onEnumMember(_enm: spec.EnumType, _member: spec.EnumMember): void;
    protected hasField(_cls: spec.ClassType, _prop: spec.Property): boolean;
    protected onField(_cls: spec.ClassType, _prop: spec.Property, _union?: spec.UnionTypeReference): void;
    private visit;
    /**
     * Adds a postfix ("XxxBase") to the class name to indicate it is abstract.
     */
    private addAbstractPostfixToClassName;
    protected excludeType(...names: string[]): void;
    private shouldExcludeType;
    /**
     * Returns all the method overloads needed to satisfy optional arguments.
     * For example, for the method `foo(bar: string, hello?: number, world?: number)`
     * this method will return:
     *  - foo(bar: string)
     *  - foo(bar: string, hello: number)
     *
     * Notice that the method that contains all the arguments will not be returned.
     */
    protected createOverloadsForOptionals<T extends spec.Method | spec.Initializer>(method: T): T[];
    private visitInterface;
    private visitClass;
    /**
     * Magical heuristic to determine which type in a union is the primary type. The primary type will not have
     * a postfix with the name of the type attached to the expanded property name.
     *
     * The primary type is determined according to the following rules (first match):
     * 1. The first primitive type
     * 2. The first primitive collection
     * 3. No primary
     */
    protected isPrimaryExpandedUnionProperty(ref: spec.UnionTypeReference | undefined, index: number): boolean;
    private visitEnum;
    private displayNameForType;
    /**
     * Looks up a jsii module in the dependency tree.
     * @param name The name of the jsii module to look up
     */
    protected findModule(name: string): spec.AssemblyConfiguration;
    protected findType(fqn: string): spec.Type;
}
//# sourceMappingURL=generator.d.ts.map