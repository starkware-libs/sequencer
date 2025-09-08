import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Docs, Documentable } from './docs';
import { OptionalValue } from './optional-value';
import { Overridable } from './overridable';
import { SourceLocatable, SourceLocation } from './source';
import { Type } from './type';
import { MemberKind, TypeMember } from './type-member';
import { TypeSystem } from './type-system';
export declare class Property extends OptionalValue implements Documentable, Overridable, TypeMember, SourceLocatable {
    readonly assembly: Assembly;
    readonly parentType: Type;
    readonly definingType: Type;
    readonly spec: jsii.Property;
    readonly kind = MemberKind.Property;
    constructor(system: TypeSystem, assembly: Assembly, parentType: Type, definingType: Type, spec: jsii.Property);
    toString(): string;
    /**
     * The name of the property.
     */
    get name(): string;
    /**
     * Indicates if this property only has a getter (immutable).
     */
    get immutable(): boolean;
    /**
     * Indicates if this property is protected (otherwise it is public)
     */
    get protected(): boolean;
    /**
     * Indicates if this property is abstract
     */
    get abstract(): boolean;
    /**
     * Indicates if this is a static property.
     */
    get static(): boolean;
    /**
     * A hint that indicates that this static, immutable property is initialized
     * during startup. This allows emitting "const" idioms in different target languages.
     * Implies `static` and `immutable`.
     */
    get const(): boolean;
    get overrides(): Type | undefined;
    get docs(): Docs;
    /**
     * Return the location in the module
     */
    get locationInModule(): SourceLocation | undefined;
    /**
     * Return the location in the repository
     */
    get locationInRepository(): SourceLocation | undefined;
}
//# sourceMappingURL=property.d.ts.map