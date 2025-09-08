import { Assembly } from '@jsii/spec';
import * as ts from 'typescript';
import { JsiiDiagnostic } from '../jsii-diagnostic';
export declare class DeprecatedRemover {
    private readonly typeChecker;
    private readonly allowlistedDeprecations;
    private readonly transformations;
    private readonly nodesToRemove;
    constructor(typeChecker: ts.TypeChecker, allowlistedDeprecations: Set<string> | undefined);
    /**
     * Obtains the configuration for the TypeScript transform(s) that will remove
     * `@deprecated` members from the generated declarations (`.d.ts`) files. It
     * will leverage information accumulated during `#removeFrom(Assembly)` in
     * order to apply corrections to inheritance chains, ensuring a valid output
     * is produced.
     */
    get customTransformers(): ts.CustomTransformers;
    /**
     * Removes all `@deprecated` API elements from the provided assembly, and
     * records the operations needed in order to fix the inheritance chains that
     * mix `@deprecated` and non-`@deprecated` types.
     *
     * @param assembly the assembly to be modified.
     *
     * @returns diagnostic messages produced when validating no remaining API
     *          makes use of a `@deprecated` type that was removed.
     */
    removeFrom(assembly: Assembly): readonly JsiiDiagnostic[];
    private findLeftoverUseOfDeprecatedAPIs;
    private verifyCallable;
    private verifyProperty;
    /**
     * Determines whether a `TypeReference` contains an FQN within a given set.
     *
     * @param ref  the tested `TypeReference`.
     * @param fqns the set of FQNs that are being searched for.
     *
     * @returns the first FQN that was identified.
     */
    private tryFindReference;
    private shouldFqnBeStripped;
    private makeDiagnostic;
}
//# sourceMappingURL=deprecated-remover.d.ts.map