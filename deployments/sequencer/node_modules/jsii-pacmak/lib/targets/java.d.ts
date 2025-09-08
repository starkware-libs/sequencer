import * as spec from '@jsii/spec';
import { RosettaTabletReader } from 'jsii-rosetta';
import { TargetBuilder, BuildOptions } from '../builder';
import { Generator } from '../generator';
import { JsiiModule } from '../packaging';
import { PackageInfo, Target, TargetOptions } from '../target';
/**
 * Build Java packages all together, by generating an aggregate POM
 *
 * This will make the Java build a lot more efficient (~300%).
 *
 * Do this by copying the code into a temporary directory, generating an aggregate
 * POM there, and then copying the artifacts back into the respective output
 * directories.
 */
export declare class JavaBuilder implements TargetBuilder {
    private readonly modules;
    private readonly options;
    private readonly targetName;
    constructor(modules: readonly JsiiModule[], options: BuildOptions);
    buildModules(): Promise<void>;
    private generateModuleCode;
    private generateAggregateSourceDir;
    private generateAggregatePom;
    private copyOutArtifacts;
    /**
     * Decide whether or not to append 'java' to the given output directory
     */
    private outputDir;
    /**
     * Generates maven settings file for this build.
     * @param where The generated sources directory. This is where user.xml will be placed.
     * @param currentOutputDirectory The current output directory. Will be added as a local maven repo.
     */
    private generateMavenSettingsForLocalDeps;
    private makeTarget;
}
export default class Java extends Target {
    static toPackageInfos(assm: spec.Assembly): {
        [language: string]: PackageInfo;
    };
    static toNativeReference(type: spec.Type, options: any): {
        java: string;
    };
    protected readonly generator: JavaGenerator;
    constructor(options: TargetOptions);
    build(sourceDir: string, outDir: string): Promise<void>;
}
declare class JavaGenerator extends Generator {
    private static readonly RESERVED_KEYWORDS;
    /**
     * Turns a raw javascript property name (eg: 'default') into a safe Java property name (eg: 'defaultValue').
     * @param propertyName the raw JSII property Name
     */
    private static safeJavaPropertyName;
    /**
     * Turns a raw javascript method name (eg: 'import') into a safe Java method name (eg: 'doImport').
     * @param methodName
     */
    private static safeJavaMethodName;
    /** If false, @Generated will not include generator version nor timestamp */
    private emitFullGeneratorInfo?;
    private moduleClass;
    /**
     * A map of all the modules ever referenced during code generation. These include
     * direct dependencies but can potentially also include transitive dependencies, when,
     * for example, we need to refer to their types when flatting the class hierarchy for
     * interface proxies.
     */
    private readonly referencedModules;
    private readonly rosetta;
    constructor(options: {
        readonly rosetta: RosettaTabletReader;
        readonly runtimeTypeChecking: boolean;
    });
    protected onBeginAssembly(assm: spec.Assembly, fingerprint: boolean): void;
    protected onEndAssembly(assm: spec.Assembly, fingerprint: boolean): void;
    protected getAssemblyOutputDir(mod: spec.Assembly): string;
    protected onBeginClass(cls: spec.ClassType, abstract: boolean): void;
    protected onEndClass(cls: spec.ClassType): void;
    protected onInitializer(cls: spec.ClassType, method: spec.Initializer): void;
    protected onInitializerOverload(cls: spec.ClassType, overload: spec.Method, _originalInitializer: spec.Method): void;
    protected onField(_cls: spec.ClassType, _prop: spec.Property, _union?: spec.UnionTypeReference): void;
    protected onProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onStaticProperty(cls: spec.ClassType, prop: spec.Property): void;
    /**
     * Since we expand the union setters, we will use this event to only emit the getter which returns an Object.
     */
    protected onUnionProperty(cls: spec.ClassType, prop: spec.Property, _union: spec.UnionTypeReference): void;
    protected onMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onMethodOverload(cls: spec.ClassType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onStaticMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onStaticMethodOverload(cls: spec.ClassType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onBeginEnum(enm: spec.EnumType): void;
    protected onEndEnum(enm: spec.EnumType): void;
    protected onEnumMember(parentType: spec.EnumType, member: spec.EnumMember): void;
    /**
     * Namespaces are handled implicitly by onBeginClass().
     *
     * Only emit package-info in case this is a submodule
     */
    protected onBeginNamespace(ns: string): void;
    protected onEndNamespace(_ns: string): void;
    protected onBeginInterface(ifc: spec.InterfaceType): void;
    protected onEndInterface(ifc: spec.InterfaceType): void;
    protected onInterfaceMethod(ifc: spec.InterfaceType, method: spec.Method): void;
    protected onInterfaceMethodOverload(ifc: spec.InterfaceType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onInterfaceProperty(ifc: spec.InterfaceType, prop: spec.Property): void;
    /**
     * Emits a local default implementation for optional properties inherited from
     * multiple distinct parent types. This remvoes the default method dispatch
     * ambiguity that would otherwise exist.
     *
     * @param ifc            the interface to be processed.
  
     *
     * @see https://github.com/aws/jsii/issues/2256
     */
    private emitMultiplyInheritedOptionalProperties;
    private emitAssemblyPackageInfo;
    private emitSubmodulePackageInfo;
    private emitMavenPom;
    private emitStaticInitializer;
    private renderConstName;
    private emitConstProperty;
    private emitProperty;
    /**
     * Filters types from a union to select only those that correspond to the
     * specified javaType.
     *
     * @param ref the type to be filtered.
     * @param javaType the java type that is expected.
     * @param covariant whether collections should use the covariant form.
     * @param optional whether the type at an optional location or not
     *
     * @returns a type reference that matches the provided javaType.
     */
    private filterType;
    private emitMethod;
    /**
     * Emits type checks for values passed for type union parameters.
     *
     * @param parameters the list of parameters received by the function.
     */
    private emitUnionParameterValdation;
    /**
     * We are now going to build a class that can be used as a proxy for untyped
     * javascript objects that implement this interface. we want java code to be
     * able to interact with them, so we will create a proxy class which
     * implements this interface and has the same methods.
     *
     * These proxies are also used to extend abstract classes to allow the JSII
     * engine to instantiate an abstract class in Java.
     */
    private emitProxy;
    private emitDefaultImplementation;
    private emitStabilityAnnotations;
    private toJavaProp;
    private emitClassBuilder;
    private emitBuilderSetter;
    private emitInterfaceBuilder;
    private emitDataType;
    private emitEqualsOverride;
    private emitHashCodeOverride;
    private openFileIfNeeded;
    private closeFileIfNeeded;
    private isNested;
    private toJavaFilePath;
    private toJavaResourcePath;
    private addJavaDocs;
    private getClassBase;
    private toDecoratedJavaType;
    private toDecoratedJavaTypes;
    private toJavaTypeNoGenerics;
    private toJavaType;
    private toNativeType;
    private toJavaTypes;
    private toJavaCollection;
    private toJavaPrimitive;
    private renderMethodCallArguments;
    private renderMethodCall;
    /**
     * Wraps a collection into an unmodifiable collection else returns the existing statement.
     * @param statement The statement to wrap if necessary.
     * @param type The type of the object to wrap.
     * @param optional Whether the value is optional (can be null/undefined) or not.
     * @returns The modified or original statement.
     */
    private wrapCollection;
    private renderMethodParameters;
    private renderAccessLevel;
    private makeModuleClass;
    private emitModuleFile;
    private emitJsiiInitializers;
    /**
     * Computes the java FQN for a JSII FQN:
     * 1. Determine which assembly the FQN belongs to (first component of the FQN)
     * 2. Locate the `targets.java.package` value for that assembly (this assembly, or one of the dependencies)
     * 3. Return the java FQN: ``<module.targets.java.package>.<FQN stipped of first component>``
     *
     * Records an assembly reference if the referenced FQN comes from a different assembly.
     *
     * @param fqn the JSII FQN to be used.
     *
     * @returns the corresponding Java FQN.
     *
     * @throws if the assembly the FQN belongs to does not have a `targets.java.package` set.
     */
    private toNativeFqn;
    /**
     * Computes Java name for a jsii assembly or type.
     *
     * @param assm The assembly that contains the type
     * @param type The type we want the name of
     */
    private toNativeName;
    /**
     * Emits an ``@Generated`` annotation honoring the ``this.emitFullGeneratorInfo`` setting.
     */
    private emitGeneratedAnnotation;
    private convertExample;
    private convertSamplesInMarkdown;
    /**
     * Fins the Java FQN of the default implementation interfaces that should be implemented when a new
     * default interface or proxy class is being emitted.
     *
     * @param type the type for which a default interface or proxy is emitted.
     * @param includeThisType whether this class's default interface should be included or not.
     *
     * @returns a list of Java fully qualified class names.
     */
    private defaultInterfacesFor;
}
export {};
//# sourceMappingURL=java.d.ts.map