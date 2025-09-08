import * as ts from 'typescript';
/**
 * Provides a TransformerFactory to annotate classes with runtime information
 * (e.g., fully-qualified name, version).
 *
 * It does this by first inserting this definition at the top of each source file:
 * ```
 * var JSII_RTTI_SYMBOL_1 = Symbol.for("jsii.rtti");
 * ```
 *
 * Then, for each class that has registered runtime information during assembly,
 * insert a static member to the class with its fqn and version:
 * ```
 * private static readonly [JSII_RTTI_SYMBOL_1] = { fqn: "ModuleName.ClassName", version: "1.2.3" }
 * ```
 */
export declare class RuntimeTypeInfoInjector {
    private readonly version;
    private readonly fqnsByClass;
    constructor(version: string);
    /**
     * Register the fully-qualified name (fqn) of a class with its ClassDeclaration.
     * Only ClassDeclarations with registered fqns will be annotated.
     */
    registerClassFqn(clazz: ts.ClassDeclaration, fqn: string): void;
    /**
     * Return the set of Transformers to be used in TSC's program.emit()
     */
    makeTransformers(): ts.CustomTransformers;
    runtimeTypeTransformer(): ts.TransformerFactory<ts.SourceFile>;
    /** Used instead of direct access to the map to faciliate testing. */
    protected getClassFqn(clazz: ts.ClassDeclaration): string | undefined;
    /**
     * If the ClassDeclaration has an associated fully-qualified name registered,
     * will append a static property to the class with the fqn and version.
     */
    private addRuntimeInfoToClass;
}
//# sourceMappingURL=runtime-info.d.ts.map