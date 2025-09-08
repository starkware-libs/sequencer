import { GoTypeMember } from '../types';
export declare abstract class FunctionCall {
    readonly parent: GoTypeMember;
    constructor(parent: GoTypeMember);
    protected get returnsVal(): boolean;
    protected get returnType(): string;
}
//# sourceMappingURL=function-call.d.ts.map