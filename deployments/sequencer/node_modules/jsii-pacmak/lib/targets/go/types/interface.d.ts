import { InterfaceType, Method, Property } from 'jsii-reflect';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { Package } from '../package';
import { MethodCall } from '../runtime';
import { ParameterValidator } from '../runtime/runtime-type-checking';
import { GoType } from './go-type';
import { GoMethod, GoProperty } from './type-member';
export declare class GoInterface extends GoType<InterfaceType> {
    #private;
    readonly methods: InterfaceMethod[];
    readonly reimplementedMethods: readonly InterfaceMethod[];
    readonly properties: InterfaceProperty[];
    readonly reimplementedProperties: readonly InterfaceProperty[];
    constructor(pkg: Package, type: InterfaceType);
    get parameterValidators(): readonly ParameterValidator[];
    emit(context: EmitContext): void;
    emitRegistration({ code }: EmitContext): void;
    get specialDependencies(): SpecialDependencies;
    get extends(): GoInterface[];
    get extendsDependencies(): Package[];
    get dependencies(): Package[];
}
declare class InterfaceProperty extends GoProperty {
    readonly parent: GoInterface;
    readonly property: Property;
    constructor(parent: GoInterface, property: Property);
    get returnType(): string;
    emit({ code, documenter }: EmitContext): void;
}
declare class InterfaceMethod extends GoMethod {
    readonly parent: GoInterface;
    readonly method: Method;
    readonly runtimeCall: MethodCall;
    constructor(parent: GoInterface, method: Method);
    emitDecl({ code, documenter }: EmitContext): void;
    emit(context: EmitContext): void;
    get specialDependencies(): SpecialDependencies;
    private get returnTypeString();
}
export {};
//# sourceMappingURL=interface.d.ts.map