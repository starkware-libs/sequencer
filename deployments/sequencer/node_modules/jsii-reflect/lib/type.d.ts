import * as jsii from '@jsii/spec';
import { Assembly } from './assembly';
import { ClassType } from './class';
import { Docs, Documentable } from './docs';
import { EnumType } from './enum';
import { InterfaceType } from './interface';
import { SourceLocatable, SourceLocation } from './source';
import { TypeReference } from './type-ref';
import { TypeSystem } from './type-system';
export declare abstract class Type implements Documentable, SourceLocatable {
    readonly system: TypeSystem;
    readonly assembly: Assembly;
    readonly spec: jsii.Type;
    constructor(system: TypeSystem, assembly: Assembly, spec: jsii.Type);
    toString(): string;
    /**
     * The fully qualified name of the type (``<assembly>.<namespace>.<name>``)
     */
    get fqn(): string;
    /**
     * The namespace of the type (``foo.bar.baz``). When undefined, the type is located at the root of the assembly
     * (it's ``fqn`` would be like ``<assembly>.<name>``). If the `namespace` corresponds to an existing type's
     * namespace-qualified (e.g: ``<namespace>.<name>``), then the current type is a nested type.
     */
    get namespace(): string | undefined;
    /**
     * The type within which this type is nested (if any).
     */
    get nestingParent(): Type | undefined;
    /**
     * The simple name of the type (MyClass).
     */
    get name(): string;
    /**
     * The kind of the type.
     */
    get kind(): jsii.TypeKind;
    get docs(): Docs;
    /**
     * A type reference to this type
     */
    get reference(): TypeReference;
    /**
     * Determines whether this is a Class type or not.
     */
    isClassType(): this is ClassType;
    /**
     * Determines whether this is a Data Type (that is, an interface with no methods) or not.
     */
    isDataType(): this is InterfaceType;
    /**
     * Determines whether this is an Enum type or not.
     */
    isEnumType(): this is EnumType;
    /**
     * Determines whether this is an Interface type or not.
     */
    isInterfaceType(): this is InterfaceType;
    /**
     * Determines whether this type extends a given base or not.
     *
     * @param base the candidate base type.
     */
    extends(base: Type): boolean;
    /**
     * Finds all type that:
     * - extend this, if this is a ClassType
     * - implement this, if this is an InterfaceType (this includes interfaces extending this)
     *
     * As classes and interfaces are considered to extend themselves, "this" will be part of all return values when called
     * on classes and interfaces.
     *
     * The result will always be empty for types that are neither ClassType nor InterfaceType.
     */
    get allImplementations(): Type[];
    /**
     * Return the location in the module
     */
    get locationInModule(): SourceLocation | undefined;
    /**
     * Return the location in the repository
     */
    get locationInRepository(): SourceLocation | undefined;
}
//# sourceMappingURL=type.d.ts.map