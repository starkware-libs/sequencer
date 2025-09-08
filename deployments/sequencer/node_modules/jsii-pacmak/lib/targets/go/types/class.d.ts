import { Method, ClassType, Initializer } from 'jsii-reflect';
import { SpecialDependencies } from '../dependencies';
import { EmitContext } from '../emit-context';
import { Package } from '../package';
import { MethodCall } from '../runtime';
import { ParameterValidator } from '../runtime/runtime-type-checking';
import { GoType } from './go-type';
import { GoInterface } from './interface';
import { GoMethod, GoProperty, GoTypeMember } from './type-member';
export declare class GoClass extends GoType<ClassType> {
    #private;
    readonly methods: ClassMethod[];
    readonly staticMethods: StaticMethod[];
    readonly properties: GoProperty[];
    readonly staticProperties: GoProperty[];
    private _extends?;
    private _implements?;
    private readonly initializer?;
    constructor(pkg: Package, type: ClassType);
    get parameterValidators(): readonly ParameterValidator[];
    get extends(): GoClass | undefined;
    get implements(): readonly GoInterface[];
    get baseTypes(): ReadonlyArray<GoClass | GoInterface>;
    emit(context: EmitContext): void;
    emitRegistration({ code }: EmitContext): void;
    get members(): GoTypeMember[];
    get specialDependencies(): SpecialDependencies;
    protected emitInterface(context: EmitContext): void;
    private emitGetters;
    private emitStruct;
    private emitSetters;
    get dependencies(): Package[];
    get interfaces(): string[];
}
export declare class GoClassConstructor extends GoMethod {
    #private;
    readonly parent: GoClass;
    private readonly type;
    private readonly constructorRuntimeCall;
    constructor(parent: GoClass, type: Initializer);
    get validator(): ParameterValidator | undefined;
    get specialDependencies(): SpecialDependencies;
    emit(context: EmitContext): void;
    private emitNew;
    private emitOverride;
}
export declare class ClassMethod extends GoMethod {
    readonly parent: GoClass;
    readonly method: Method;
    readonly runtimeCall: MethodCall;
    constructor(parent: GoClass, method: Method);
    emit(context: EmitContext): void;
    emitDecl({ code, documenter }: EmitContext): void;
    get instanceArg(): string;
    get static(): boolean;
    get specialDependencies(): SpecialDependencies;
}
export declare class StaticMethod extends ClassMethod {
    readonly parent: GoClass;
    readonly method: Method;
    readonly name: string;
    constructor(parent: GoClass, method: Method);
    emit(context: EmitContext): void;
}
//# sourceMappingURL=class.d.ts.map