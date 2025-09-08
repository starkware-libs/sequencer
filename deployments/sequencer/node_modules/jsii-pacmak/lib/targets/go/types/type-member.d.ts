import { Callable, Parameter, Property } from 'jsii-reflect';
import { ApiLocation } from 'jsii-rosetta';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { ParameterValidator } from '../runtime/runtime-type-checking';
import { GoClass, GoType, GoInterface, GoTypeRef } from './index';
export interface GoTypeMember {
    name: string;
    parent: GoType;
    reference?: GoTypeRef;
    returnType: string;
    specialDependencies: SpecialDependencies;
}
export declare class GoProperty implements GoTypeMember {
    #private;
    parent: GoType;
    readonly property: Property;
    readonly name: string;
    readonly setterName: string;
    readonly immutable: boolean;
    protected readonly apiLocation: ApiLocation;
    constructor(parent: GoType, property: Property);
    get validator(): ParameterValidator | undefined;
    get reference(): GoTypeRef;
    get specialDependencies(): SpecialDependencies;
    get static(): boolean;
    get returnType(): string;
    get instanceArg(): string;
    get override(): string;
    emitStructMember({ code, documenter }: EmitContext): void;
    emitGetterDecl({ code, documenter }: EmitContext): void;
    emitSetterDecl({ code, documenter }: EmitContext): void;
    emitGetterProxy(context: EmitContext): void;
    emitSetterProxy(context: EmitContext): void;
}
export declare abstract class GoMethod implements GoTypeMember {
    #private;
    readonly parent: GoClass | GoInterface;
    readonly method: Callable;
    readonly name: string;
    readonly parameters: GoParameter[];
    protected readonly apiLocation: ApiLocation;
    constructor(parent: GoClass | GoInterface, method: Callable);
    get validator(): ParameterValidator | undefined;
    abstract emit(context: EmitContext): void;
    abstract get specialDependencies(): SpecialDependencies;
    get reference(): GoTypeRef | undefined;
    get returnsRef(): boolean;
    get returnType(): string;
    get instanceArg(): string;
    get override(): string;
    get static(): boolean;
    paramString(): string;
}
export declare class GoParameter {
    readonly name: string;
    readonly isOptional: boolean;
    readonly isVariadic: boolean;
    private readonly type;
    private readonly pkg;
    constructor(parent: GoClass | GoInterface, parameter: Parameter);
    get reference(): GoTypeRef;
    toString(): string;
}
//# sourceMappingURL=type-member.d.ts.map