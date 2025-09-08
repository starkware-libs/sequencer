import { EnumType } from 'jsii-reflect';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { Package } from '../package';
import { GoType } from './go-type';
export declare class Enum extends GoType<EnumType> {
    private readonly members;
    constructor(pkg: Package, type: EnumType);
    get parameterValidators(): never[];
    emit(context: EmitContext): void;
    emitRegistration({ code }: EmitContext): void;
    get dependencies(): Package[];
    get specialDependencies(): SpecialDependencies;
}
//# sourceMappingURL=enum.d.ts.map