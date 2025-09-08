import * as spec from '@jsii/spec';
import { DotNetDependency } from './filegenerator';
type FindModuleCallback = (fqn: string) => spec.AssemblyConfiguration;
type FindTypeCallback = (fqn: string) => spec.Type;
export declare class DotNetTypeResolver {
    private readonly assembliesCurrentlyBeingCompiled;
    namespaceDependencies: Map<string, DotNetDependency>;
    private readonly findModule;
    private readonly findType;
    private readonly assembly;
    private readonly nameutils;
    constructor(assembly: spec.Assembly, findModule: FindModuleCallback, findType: FindTypeCallback, assembliesCurrentlyBeingCompiled: string[]);
    /**
     * Translates a type fqn to a native .NET full type
     */
    toNativeFqn(fqn: string): string;
    /**
     * Resolves the namespaces dependencies by looking at the .jsii model
     */
    resolveNamespacesDependencies(): void;
    /**
     * Loops through the implemented interfaces and returns the fully qualified .NET types of the interfaces
     *
     */
    resolveImplementedInterfaces(ifc: spec.InterfaceType | spec.ClassType): string[];
    /**
     * Translates any jsii type to its corresponding .NET type
     */
    toDotNetType(typeref: spec.TypeReference): string;
    /**
     * Translates any jsii type to the name of its corresponding .NET type (as a .NET string).
     */
    toDotNetTypeName(typeref: spec.TypeReference): string;
    resolveNamespace(assm: spec.AssemblyConfiguration, assmName: string, ns: string): string;
    /**
     * Translates a primitive in jsii to a native .NET primitive
     */
    private toDotNetPrimitive;
    /**
     * Translates a primitive in jsii to the name of a native .NET primitive
     */
    private toDotNetPrimitiveName;
    /**
     * Translates a collection in jsii to a native .NET collection
     */
    private toDotNetCollection;
    /**
     * Translates a collection in jsii to the name of a native .NET collection
     */
    private toDotNetCollectionName;
}
export {};
//# sourceMappingURL=dotnettyperesolver.d.ts.map