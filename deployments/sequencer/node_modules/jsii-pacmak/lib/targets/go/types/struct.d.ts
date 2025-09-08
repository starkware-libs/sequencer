import { InterfaceType } from 'jsii-reflect';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { Package } from '../package';
import { ParameterValidator, StructValidator } from '../runtime/runtime-type-checking';
import { GoType } from './go-type';
import { GoProperty } from './type-member';
export declare class Struct extends GoType<InterfaceType> {
    #private;
    readonly properties: readonly GoProperty[];
    constructor(parent: Package, type: InterfaceType);
    get parameterValidators(): readonly ParameterValidator[];
    get structValidator(): StructValidator | undefined;
    get dependencies(): Package[];
    get specialDependencies(): SpecialDependencies;
    emit(context: EmitContext): void;
    emitRegistration({ code, runtimeTypeChecking }: EmitContext): void;
}
//# sourceMappingURL=struct.d.ts.map