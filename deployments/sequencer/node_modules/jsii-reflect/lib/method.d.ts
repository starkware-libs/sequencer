import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { Callable } from './callable';
import { Documentable } from './docs';
import { OptionalValue } from './optional-value';
import { Overridable } from './overridable';
import { SourceLocatable } from './source';
import { Type } from './type';
import { MemberKind, TypeMember } from './type-member';
import { TypeSystem } from './type-system';
/**
 * Symbolic name for the constructor
 */
export declare const INITIALIZER_NAME = "<initializer>";
export declare class Method extends Callable implements Documentable, Overridable, TypeMember, SourceLocatable {
    readonly definingType: Type;
    readonly spec: jsii.Method;
    static isMethod(x: Callable): x is Method;
    readonly kind = MemberKind.Method;
    constructor(system: TypeSystem, assembly: Assembly, parentType: Type, definingType: Type, spec: jsii.Method);
    /**
     * The name of the method.
     */
    get name(): string;
    get overrides(): Type | undefined;
    /**
     * The return type of the method (undefined if void or initializer)
     */
    get returns(): OptionalValue;
    /**
     * Is this method an abstract method (this means the class will also be an abstract class)
     */
    get abstract(): boolean;
    /**
     * Is this method asyncrhonous (this means the return value is a promise)
     */
    get async(): boolean;
    /**
     * Indicates if this is a static method.
     */
    get static(): boolean;
}
//# sourceMappingURL=method.d.ts.map