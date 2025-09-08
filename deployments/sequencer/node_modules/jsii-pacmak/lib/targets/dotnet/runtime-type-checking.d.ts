import { CodeMaker } from 'codemaker';
import { Parameter } from 'jsii-reflect';
import { DotNetTypeResolver } from './dotnettyperesolver';
import { DotNetNameUtils } from './nameutils';
export declare class ParameterValidator {
    private readonly validations;
    static forParameters(parameters: readonly Parameter[], nameUtils: DotNetNameUtils, { noMangle }: {
        readonly noMangle: boolean;
    }): ParameterValidator | undefined;
    private constructor();
    emit(code: CodeMaker, resolver: DotNetTypeResolver): void;
}
//# sourceMappingURL=runtime-type-checking.d.ts.map