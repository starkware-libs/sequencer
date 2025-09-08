import { CodeMaker } from 'codemaker';
import { EmitContext } from '../emit-context';
import { GoProperty } from '../types';
import { FunctionCall } from './function-call';
export declare class GetProperty extends FunctionCall {
    readonly parent: GoProperty;
    constructor(parent: GoProperty);
    emit(code: CodeMaker): void;
}
export declare class SetProperty {
    readonly parent: GoProperty;
    constructor(parent: GoProperty);
    emit({ code, runtimeTypeChecking }: EmitContext): void;
}
export declare class StaticGetProperty extends FunctionCall {
    readonly parent: GoProperty;
    constructor(parent: GoProperty);
    emit(code: CodeMaker): void;
}
export declare class StaticSetProperty {
    readonly parent: GoProperty;
    constructor(parent: GoProperty);
    emit({ code, runtimeTypeChecking }: EmitContext): void;
}
//# sourceMappingURL=property-access.d.ts.map