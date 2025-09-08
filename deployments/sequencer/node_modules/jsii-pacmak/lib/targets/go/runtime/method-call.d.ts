import { EmitContext } from '../emit-context';
import { GoMethod } from '../types';
import { FunctionCall } from './function-call';
export declare class MethodCall extends FunctionCall {
    readonly parent: GoMethod;
    private _returnVarName;
    constructor(parent: GoMethod);
    emit(context: EmitContext): void;
    private emitDynamic;
    private emitStatic;
    private get returnVarName();
    private get inStatic();
}
//# sourceMappingURL=method-call.d.ts.map