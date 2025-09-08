import { CodeMaker } from 'codemaker';
import { SpecialDependencies } from '../dependencies';
import { Package } from '../package';
import { GoClassConstructor, GoMethod, GoProperty, Struct } from '../types';
export declare class ParameterValidator {
    static forConstructor(ctor: GoClassConstructor): ParameterValidator | undefined;
    static forMethod(method: GoMethod): ParameterValidator | undefined;
    static forProperty(property: GoProperty): ParameterValidator | undefined;
    private static fromParts;
    private readonly receiver?;
    private readonly name;
    private readonly parameters;
    private readonly validations;
    private constructor();
    get dependencies(): readonly Package[];
    get specialDependencies(): SpecialDependencies;
    emitCall(code: CodeMaker): void;
    emitImplementation(code: CodeMaker, scope: Package, noOp?: boolean): void;
}
export declare class StructValidator {
    private readonly receiver;
    private readonly validations;
    static for(struct: Struct): StructValidator | undefined;
    private constructor();
    get dependencies(): Package[];
    get specialDependencies(): SpecialDependencies;
    emitImplementation(code: CodeMaker, scope: Package, noOp?: boolean): void;
}
//# sourceMappingURL=runtime-type-checking.d.ts.map