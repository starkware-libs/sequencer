import { CodeMaker } from 'codemaker';
import { Type } from 'jsii-reflect';
import { ApiLocation } from 'jsii-rosetta';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { Package } from '../package';
import { GoClass } from './class';
import { GoInterface } from './interface';
import { ParameterValidator, StructValidator } from '../runtime/runtime-type-checking';
export declare abstract class GoType<T extends Type = Type> {
    readonly pkg: Package;
    readonly type: T;
    readonly name: string;
    readonly fqn: string;
    readonly proxyName: string;
    protected readonly apiLocation: ApiLocation;
    constructor(pkg: Package, type: T);
    get structValidator(): StructValidator | undefined;
    abstract get parameterValidators(): readonly ParameterValidator[];
    abstract emit(context: EmitContext): void;
    abstract emitRegistration(context: EmitContext): void;
    abstract get dependencies(): Package[];
    abstract get specialDependencies(): SpecialDependencies;
    get namespace(): string;
    emitDocs(context: EmitContext): void;
    protected emitStability(context: EmitContext): void;
    protected emitProxyMakerFunction(code: CodeMaker, bases: ReadonlyArray<GoClass | GoInterface>): void;
}
//# sourceMappingURL=go-type.d.ts.map