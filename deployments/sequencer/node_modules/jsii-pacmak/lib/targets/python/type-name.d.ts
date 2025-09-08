import { Assembly, OptionalValue, TypeReference, Type } from '@jsii/spec';
export interface TypeName {
    pythonType(context: NamingContext): string;
    requiredImports(context: NamingContext): PythonImports;
}
export interface PythonImports {
    /**
     * For a given source module, what elements to import. The empty string value
     * indicates a need to import the module fully ("import <name>") instead of
     * doing a piecemeal import ("from <name> import <item>").
     */
    readonly [sourcePackage: string]: ReadonlySet<string>;
}
/**
 * The context in which a PythonType is being considered.
 */
export interface NamingContext {
    /** The assembly in which the PythonType is expressed. */
    readonly assembly: Assembly;
    /** A resolver to obtain complete information about a type. */
    readonly typeResolver: (fqn: string) => Type;
    /** The submodule of the assembly in which the PythonType is expressed (could be the module root) */
    readonly submodule: string;
    /**
     * The declaration is made in the context of a type annotation (so it can be quoted)
     *
     * @default true
     */
    readonly typeAnnotation?: boolean;
    /**
     * A an array representing the stack of declarations currently being
     * initialized. All of these names can only be referred to using a forward
     * reference (stringified type name) in the context of type signatures (but
     * they can be used safely from implementations so long as those are not *run*
     * as part of the declaration).
     *
     * @default []
     */
    readonly surroundingTypeFqns?: readonly string[];
    /**
     * Disables generating typing.Optional wrappers
     * @default false
     * @internal
     */
    readonly ignoreOptional?: boolean;
    /**
     * The set of jsii type FQNs that have already been emitted so far. This is
     * used to determine whether a given type reference is a forward declaration
     * or not when emitting type signatures.
     */
    readonly emittedTypes: Set<string>;
    /**
     * Whether the type is emitted for a parameter or not. This may change the
     * exact type signature being emitted (e.g: Arrays are typing.Sequence[T] for
     * parameters, and typing.List[T] otherwise).
     */
    readonly parameterType?: boolean;
}
export declare function toTypeName(ref?: OptionalValue | TypeReference): TypeName;
/**
 * Obtains the Python package name for a given submodule FQN.
 *
 * @param fqn      the submodule FQN for which a package name is needed.
 * @param rootAssm the assembly this FQN belongs to.
 */
export declare function toPackageName(fqn: string, rootAssm: Assembly): string;
export declare function mergePythonImports(...pythonImports: readonly PythonImports[]): PythonImports;
export declare function toPythonFqn(fqn: string, rootAssm: Assembly): {
    assemblyName: string;
    packageName: string;
    pythonFqn: string;
};
//# sourceMappingURL=type-name.d.ts.map