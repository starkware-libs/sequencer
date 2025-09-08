import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Docs, Documentable } from './docs';
import { Overridable } from './overridable';
import { Parameter } from './parameter';
import { SourceLocatable, SourceLocation } from './source';
import { Type } from './type';
import { MemberKind, TypeMember } from './type-member';
import { TypeSystem } from './type-system';
export declare abstract class Callable implements Documentable, Overridable, TypeMember, SourceLocatable {
    readonly system: TypeSystem;
    readonly assembly: Assembly;
    readonly parentType: Type;
    readonly spec: jsii.Callable;
    abstract readonly kind: MemberKind;
    abstract readonly name: string;
    abstract readonly abstract: boolean;
    constructor(system: TypeSystem, assembly: Assembly, parentType: Type, spec: jsii.Callable);
    /**
     * The parameters of the method/initializer
     */
    get parameters(): Parameter[];
    /**
     * Indicates if this method is protected (otherwise it is public)
     */
    get protected(): boolean;
    /**
     * Indicates whether this method is variadic or not. When ``true``, the last
     * element of ``#parameters`` will also be flagged ``#variadic``.
     */
    get variadic(): boolean;
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
    toString(): string;
}
//# sourceMappingURL=callable.d.ts.map