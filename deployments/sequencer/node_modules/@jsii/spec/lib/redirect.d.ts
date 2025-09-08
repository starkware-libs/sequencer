export declare const assemblyRedirectSchema: any;
declare const SCHEMA = "jsii/file-redirect";
export interface AssemblyRedirect {
    readonly schema: typeof SCHEMA;
    /**
     * The compression applied to the target file, if any.
     */
    readonly compression?: 'gzip';
    /**
     * The name of the file the assembly is redirected to.
     */
    readonly filename: string;
}
/**
 * Checks whether the provided value is an assembly redirect. This only checks
 * for presence of the correct value in the `schema` attribute. For full
 * validation, `validateAssemblyRedirect` should be used instead.
 *
 * @param obj the value to be tested.
 *
 * @returns `true` if the value is indeed an AssemblyRedirect.
 */
export declare function isAssemblyRedirect(obj: unknown): obj is AssemblyRedirect;
/**
 * Validates the provided value as an assembly redirect.
 *
 * @param obj the value to be tested.
 *
 * @returns the validated value.
 */
export declare function validateAssemblyRedirect(obj: unknown): AssemblyRedirect;
export {};
//# sourceMappingURL=redirect.d.ts.map