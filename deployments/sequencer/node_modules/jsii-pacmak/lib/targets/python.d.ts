import * as spec from '@jsii/spec';
import { CodeMaker } from 'codemaker';
import { RosettaTabletReader, ApiLocation } from 'jsii-rosetta';
import { Generator, GeneratorOptions } from '../generator';
import { Target, TargetOptions } from '../target';
import { NamingContext, PythonImports } from './python/type-name';
export default class Python extends Target {
    protected readonly generator: PythonGenerator;
    constructor(options: TargetOptions);
    generateCode(outDir: string, tarball: string): Promise<void>;
    build(sourceDir: string, outDir: string): Promise<void>;
}
interface EmitContext extends NamingContext {
    /** @deprecated The TypeResolver */
    readonly resolver: TypeResolver;
    /** Whether to emit runtime type checking code */
    readonly runtimeTypeChecking: boolean;
    /** Whether to runtime type check keyword arguments (i.e: struct constructors) */
    readonly runtimeTypeCheckKwargs?: boolean;
    /** The numerical IDs used for type annotation data storing */
    readonly typeCheckingHelper: TypeCheckingHelper;
}
declare class TypeCheckingHelper {
    #private;
    getTypeHints(fqn: string, args: readonly string[]): string;
    /** Emits instructions that create the annotations data... */
    flushStubs(code: CodeMaker): void;
}
interface PythonBase {
    readonly pythonName: string;
    readonly docs?: spec.Docs;
    emit(code: CodeMaker, context: EmitContext, opts?: any): void;
    requiredImports(context: EmitContext): PythonImports;
}
interface PythonType extends PythonBase {
    readonly fqn?: string;
    addMember(member: PythonBase): void;
}
type FindModuleCallback = (fqn: string) => spec.AssemblyConfiguration;
type FindTypeCallback = (fqn: string) => spec.Type;
declare class TypeResolver {
    private readonly types;
    private readonly boundTo?;
    private readonly boundRe;
    private readonly moduleName?;
    private readonly moduleRe;
    private readonly findModule;
    private readonly findType;
    constructor(types: Map<string, PythonType>, findModule: FindModuleCallback, findType: FindTypeCallback, boundTo?: string, moduleName?: string);
    bind(fqn: string, moduleName?: string): TypeResolver;
    isInModule(typeRef: spec.NamedTypeReference | string): boolean;
    isInNamespace(typeRef: spec.NamedTypeReference | string): boolean;
    getParent(typeRef: spec.NamedTypeReference | string): PythonType;
    getDefiningPythonModule(typeRef: spec.NamedTypeReference | string): string;
    getType(typeRef: spec.NamedTypeReference): PythonType;
    dereference(typeRef: string | spec.NamedTypeReference): spec.Type;
    private toPythonFQN;
}
declare class PythonGenerator extends Generator {
    private readonly rosetta;
    private package;
    private rootModule?;
    private readonly types;
    constructor(rosetta: RosettaTabletReader, options: GeneratorOptions);
    emitDocString(code: CodeMaker, apiLocation: ApiLocation, docs: spec.Docs | undefined, options?: {
        arguments?: DocumentableArgument[];
        documentableItem?: string;
        trailingNewLine?: boolean;
    }): void;
    convertExample(example: string, apiLoc: ApiLocation): string;
    convertMarkdown(markdown: string, apiLoc: ApiLocation): string;
    getPythonType(fqn: string): PythonType;
    protected getAssemblyOutputDir(assm: spec.Assembly): string;
    protected onBeginAssembly(assm: spec.Assembly, _fingerprint: boolean): void;
    protected onEndAssembly(assm: spec.Assembly, _fingerprint: boolean): void;
    /**
     * Will be called for assembly root, namespaces and submodules (anything that contains other types, based on its FQN)
     */
    protected onBeginNamespace(ns: string): void;
    protected onEndNamespace(ns: string): void;
    protected onBeginClass(cls: spec.ClassType, abstract: boolean | undefined): void;
    protected onStaticMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onStaticProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onMethod(cls: spec.ClassType, method: spec.Method): void;
    protected onProperty(cls: spec.ClassType, prop: spec.Property): void;
    protected onUnionProperty(cls: spec.ClassType, prop: spec.Property, _union: spec.UnionTypeReference): void;
    protected onBeginInterface(ifc: spec.InterfaceType): void;
    protected onEndInterface(_ifc: spec.InterfaceType): void;
    protected onInterfaceMethod(ifc: spec.InterfaceType, method: spec.Method): void;
    protected onInterfaceProperty(ifc: spec.InterfaceType, prop: spec.Property): void;
    protected onBeginEnum(enm: spec.EnumType): void;
    protected onEnumMember(enm: spec.EnumType, member: spec.EnumMember): void;
    protected onInterfaceMethodOverload(_ifc: spec.InterfaceType, _overload: spec.Method, _originalMethod: spec.Method): void;
    protected onMethodOverload(_cls: spec.ClassType, _overload: spec.Method, _originalMethod: spec.Method): void;
    protected onStaticMethodOverload(_cls: spec.ClassType, _overload: spec.Method, _originalMethod: spec.Method): void;
    private getAssemblyModuleName;
    private getParentFQN;
    private getParent;
    private addPythonType;
    private getliftedProp;
    private getAbstractBases;
}
/**
 * Positional argument or keyword parameter
 */
interface DocumentableArgument {
    name: string;
    definingType: spec.Type;
    docs?: spec.Docs;
}
export {};
//# sourceMappingURL=python.d.ts.map