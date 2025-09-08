import * as spec from '@jsii/spec';
import * as reflect from 'jsii-reflect';
import { RosettaTabletReader } from 'jsii-rosetta';
import { Generator, Legalese } from '../../generator';
/**
 * CODE GENERATOR V2
 */
export declare class DotNetGenerator extends Generator {
    private readonly assembliesCurrentlyBeingCompiled;
    private readonly nameutils;
    private readonly rosetta;
    private firstMemberWritten;
    private typeresolver;
    private dotnetRuntimeGenerator;
    private dotnetDocGenerator;
    constructor(assembliesCurrentlyBeingCompiled: string[], options: {
        readonly rosetta: RosettaTabletReader;
        readonly runtimeTypeChecking: boolean;
    });
    load(packageRoot: string, assembly: reflect.Assembly): Promise<void>;
    /**
     * Runs the generator (in-memory).
     */
    generate(fingerprint: boolean): void;
    save(outdir: string, tarball: string, { license, notice }: Legalese): Promise<string[]>;
    /**
     * Generates the anchor file
     */
    protected generateDependencyAnchorFile(): void;
    /**
     * Not used as we override the save() method
     */
    protected getAssemblyOutputDir(mod: spec.Assembly): string;
    /**
     * Namespaces are handled implicitly by openFileIfNeeded().
     *
     * Do generate docs if this is for a submodule though.
     */
    protected onBeginNamespace(jsiiNs: string): void;
    protected onEndNamespace(_ns: string): void;
    protected onBeginInterface(ifc: spec.InterfaceType): void;
    protected onEndInterface(ifc: spec.InterfaceType): void;
    protected onInterfaceMethod(ifc: spec.InterfaceType, method: spec.Method): void;
    protected onInterfaceMethodOverload(ifc: spec.InterfaceType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onInterfaceProperty(ifc: spec.InterfaceType, prop: spec.Property): void;
    protected onBeginClass(cls: spec.ClassType, abstract: boolean): void;
    protected onEndClass(cls: spec.ClassType): void;
    protected onField(_cls: spec.ClassType, _prop: spec.Property, _union?: spec.UnionTypeReference): void;
    protected onMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onMethodOverload(cls: spec.ClassType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onStaticMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onStaticMethodOverload(cls: spec.ClassType, overload: spec.Method, _originalMethod: spec.Method): void;
    protected onStaticProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onUnionProperty(cls: spec.ClassType, prop: spec.Property, _union: spec.UnionTypeReference): void;
    protected onBeginEnum(enm: spec.EnumType): void;
    protected onEndEnum(enm: spec.EnumType): void;
    protected onEnumMember(enm: spec.EnumType, member: spec.EnumMember): void;
    private namespaceFor;
    private emitMethod;
    private memberKeywords;
    /**
     * Emits type checks for values passed for type union parameters.
     *
     * @param parameters the list of parameters received by the function.
     * @param noMangle   use parameter names as-is (useful for setters, for example) instead of mangling them.
     */
    private emitUnionParameterValdation;
    /**
     * Founds out if a member (property or method) is already defined in one of the base classes
     *
     * Used to figure out if the override or virtual keywords are necessary.
     */
    private isMemberDefinedOnAncestor;
    /**
     * Renders method parameters string
     */
    private renderMethodParameters;
    /**
     * Renders parameters string for methods or constructors
     */
    private renderParametersString;
    /**
     * Emits an interface proxy for an interface or an abstract class.
     */
    private emitInterfaceProxy;
    /**
     * Determines whether any ancestor of the given type must use the `new`
     * modifier when introducing it's own proxy.
     *
     * If the type is a `class`, then it must use `new` if it extends another
     * abstract class defined in the same assembly (since proxies are internal,
     * external types' proxies are not visible in that context).
     *
     * If the type is an `interface`, then it must use `new` if it extends another
     * interface from the same assembly.
     *
     * @param type the tested proxy-able type (an abstract class or an interface).
     *
     * @returns true if any ancestor of this type has a visible proxy.
     */
    private proxyMustUseNewModifier;
    /**
     * Emits an Interface Datatype class
     *
     * This is used to emit a class implementing an interface when the datatype property is true in the jsii model
     * The generation of the interface proxy may not be needed if the interface is also set as a datatype
     */
    private emitInterfaceDataType;
    /**
     * Generates the body of the interface proxy or data type class
     *
     * This loops through all the member and generates them
     */
    private emitInterfaceMembersForProxyOrDatatype;
    /**
     * Emits a property
     */
    private emitProperty;
    /**
     * Emits a (static) constant property
     */
    private emitConstProperty;
    private renderAccessLevel;
    private isNested;
    private toCSharpFilePath;
    private openFileIfNeeded;
    private closeFileIfNeeded;
    /**
     * Resets the firstMember boolean flag to keep track of the first member of a new file
     *
     * This avoids unnecessary white lines
     */
    private flagFirstMemberWritten;
    /**
     * Emits a new line prior to writing a new property, method, if the property is not the first one in the class
     *
     * This avoids unnecessary white lines.
     */
    private emitNewLineIfNecessary;
    private emitAssemblyDocs;
    /**
     * Emit an unused, empty class called `NamespaceDoc` to attach the module README to
     *
     * There is no way to attach doc comments to a namespace in C#, and this trick has been
     * semi-standardized by NDoc and Sandcastle Help File Builder.
     *
     * DocFX doesn't support it out of the box, but we should be able to get there with a
     * bit of hackery.
     *
     * In any case, we need a place to attach the docs where they can be transported around,
     * might as well be this method.
     */
    private emitNamespaceDocs;
    /**
     * Emit an attribute that will hide the subsequent API element from users
     */
    private emitHideAttribute;
}
//# sourceMappingURL=dotnetgenerator.d.ts.map