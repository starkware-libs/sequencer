import * as ts from 'typescript';
import { Emitter } from './emitter';
import { ProjectInfo } from './project-info';
/**
 * The JSII Assembler consumes a ``ts.Program`` instance and emits a JSII assembly.
 */
export declare class Assembler implements Emitter {
    readonly projectInfo: ProjectInfo;
    private readonly system;
    readonly program: ts.Program;
    readonly stdlib: string;
    private readonly runtimeTypeInfoInjector;
    private readonly deprecatedRemover?;
    private readonly warningsInjector?;
    private readonly mainFile;
    private readonly tscRootDir?;
    private readonly compressAssembly?;
    private readonly _typeChecker;
    private _diagnostics;
    private _deferred;
    private readonly _types;
    private readonly _packageInfoCache;
    /** Map of Symbol to namespace export Symbol */
    private readonly _submoduleMap;
    /**
     * Submodule information
     *
     * Contains submodule information for all namespaces that have been seen
     * across all assemblies (this and dependencies).
     *
     * Filtered to local submodules only at time of writing the assembly out to disk.
     */
    private readonly _submodules;
    /**
     * @param projectInfo information about the package being assembled
     * @param program     the TypeScript program to be assembled from
     * @param stdlib      the directory where the TypeScript stdlib is rooted
     */
    constructor(projectInfo: ProjectInfo, system: ts.System, program: ts.Program, stdlib: string, options?: AssemblerOptions);
    get customTransformers(): ts.CustomTransformers;
    /**
     * Attempt emitting the JSII assembly for the program.
     *
     * @return the result of the assembly emission.
     */
    emit(): ts.EmitResult;
    private _afterEmit;
    /**
     * Defer a callback until a (set of) types are available
     *
     * This is a helper function around _defer() which encapsulates the _dereference
     * action (which is basically the majority use case for _defer anyway).
     *
     * Will not invoke the function with any 'undefined's; an error will already have been emitted in
     * that case anyway.
     *
     * @param fqn FQN of the current type (the type that has a dependency on baseTypes)
     * @param baseTypes Array of type references to be looked up
     * @param referencingNode Node to report a diagnostic on if we fail to look up a t ype
     * @param cb Callback to be invoked with the Types corresponding to the TypeReferences in baseTypes
     */
    private _deferUntilTypesAvailable;
    /**
     * Defer checks for after the program has been entirely processed; useful for verifying type references that may not
     * have been discovered yet, and verifying properties about them.
     *
     * The callback is guaranteed to be executed only after all deferreds for all types in 'dependedFqns' have
     * been executed.
     *
     * @param fqn FQN of the current type.
     * @param dependedFqns List of FQNs of types this callback depends on. All deferreds for all
     * @param cb the function to be called in a deferred way. It will be bound with ``this``, so it can depend on using
     *           ``this``.
     */
    private _defer;
    /**
     * Obtains the ``spec.Type`` for a given ``spec.NamedTypeReference``.
     *
     * @param ref the type reference to be de-referenced
     *
     * @returns the de-referenced type, if it was found, otherwise ``undefined``.
     */
    private _dereference;
    /**
     * Compute the JSII fully qualified name corresponding to a ``ts.Type`` instance. If for any reason a name cannot be
     * computed for the type, a marker is returned instead, and an ``ts.DiagnosticCategory.Error`` diagnostic is
     * inserted in the assembler context.
     *
     * @param type the type for which a JSII fully qualified name is needed.
     * @param typeAnnotationNode the type annotation for which this FQN is generated. This is used for attaching the error
     *                           marker. When there is no explicit type annotation (e.g: inferred method return type), the
     *                           preferred substitute is the "type-inferred" element's name.
     * @param typeUse the reason why this type was resolved (e.g: "return type")
     * @param isThisType whether this type was specified or inferred as "this" or not
     *
     * @returns the FQN of the type, or some "unknown" marker.
     */
    private _getFQN;
    /**
     * For all modules in the dependency closure, crawl their exports to register
     * the submodules they contain.
     *
     * @param entryPoint the main source file for the currently compiled module.
     */
    private _registerDependenciesNamespaces;
    private _registerNamespaces;
    /**
     * Registers Symbols to a particular submodule. This is used to associate
     * declarations exported by an `export * as ns from 'moduleLike';` statement
     * so that they can subsequently be correctly namespaced.
     *
     * @param ns          the symbol that identifies the submodule.
     * @param moduleLike  the module-like symbol bound to the submodule.
     * @param packageRoot the root of the package being traversed.
     */
    private _addToSubmodule;
    /**
     * Register exported types in ``this.types``.
     *
     * @param node       a node found in a module
     * @param namePrefix the prefix for the types' namespaces
     */
    private _visitNode;
    private getSymbolId;
    private _validateHeritageClauses;
    private declarationLocation;
    private _processBaseInterfaces;
    private _visitClass;
    /**
     * Use the TypeChecker's getTypeFromTypeNode, but throw a descriptive error if it fails
     */
    private _getTypeFromTypeNode;
    /**
     * Check that this class doesn't declare any members that are of different staticness in itself or any of its bases
     */
    private _verifyNoStaticMixing;
    /**
     * Wrapper around _deferUntilTypesAvailable, invoke the callback with the given classes' base type
     *
     * Does nothing if the given class doesn't have a base class.
     *
     * The second argument will be a `recurse` function for easy recursion up the inheritance tree
     * (no messing around with binding 'self' and 'this' and doing multiple calls to _withBaseClass.)
     */
    private _withBaseClass;
    /**
     * @returns true if this member is internal and should be omitted from the type manifest
     */
    private _isPrivateOrInternal;
    private _visitEnum;
    private assertNoDuplicateEnumValues;
    /**
     * Return docs for a symbol
     */
    private _visitDocumentation;
    /**
     * Check that all parameters the doc block refers to with a @param declaration actually exist
     */
    private _validateReferencedDocParams;
    private _visitInterface;
    private _visitMethod;
    private _warnAboutReservedWords;
    private _visitProperty;
    private _toParameter;
    private _typeReference;
    private _optionalValue;
    private callDeferredsInOrder;
    /**
     * Return the set of all (inherited) properties of an interface
     */
    private allProperties;
    private _verifyConsecutiveOptionals;
    /**
     * Updates the runtime type info with the fully-qualified name for the current class definition.
     * Used by the runtime type info injector to add this information to the compiled file.
     */
    private registerExportedClassFqn;
    /**
     * Return only those submodules from the submodules list that are submodules inside this
     * assembly.
     */
    private mySubmodules;
    private findPackageInfo;
}
export interface AssemblerOptions {
    /**
     * Whether to remove `@deprecated` members from the generated assembly.
     *
     * @default false
     */
    readonly stripDeprecated?: boolean;
    /**
     * If `stripDeprecated` is true, and a file is provided here, only the FQNs
     * present in the file will actually be removed. This can be useful when
     * you wish to deprecate some elements without actually removing them.
     *
     * @default undefined
     */
    readonly stripDeprecatedAllowListFile?: string;
    /**
     * Whether to inject code that warns when a deprecated element is used.
     *
     * @default false
     */
    readonly addDeprecationWarnings?: boolean;
    /**
     * Whether to compress the assembly.
     *
     * @default false
     */
    readonly compressAssembly?: boolean;
}
//# sourceMappingURL=assembler.d.ts.map