import * as jsii from '@jsii/spec';
import { TypeSystem } from './type-system';
export interface Documentable {
    docs: Docs;
}
export declare class Docs {
    readonly system: TypeSystem;
    readonly target: Documentable;
    private readonly parentDocs?;
    readonly docs: jsii.Docs;
    constructor(system: TypeSystem, target: Documentable, spec: jsii.Docs, parentDocs?: Docs | undefined);
    /**
     * Returns docstring of summary and remarks
     */
    toString(): string;
    get subclassable(): boolean;
    /**
     * Return the reason for deprecation of this type
     */
    get deprecationReason(): string | undefined;
    /**
     * Return whether this type is deprecated
     */
    get deprecated(): boolean;
    /**
     * Return the stability of this type
     */
    get stability(): jsii.Stability | undefined;
    /**
     * Return any custom tags on this type
     */
    customTag(tag: string): string | undefined;
    /**
     * Return summary of this type
     */
    get summary(): string;
    /**
     * Return remarks for this type
     */
    get remarks(): string;
    /**
     * Return examples for this type
     */
    get example(): string;
    /**
     * Return documentation links for this type
     */
    get link(): string;
    /**
     * Returns the return type
     */
    get returns(): string;
    /**
     * Returns the default value
     */
    get default(): string;
}
//# sourceMappingURL=docs.d.ts.map