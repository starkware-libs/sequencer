import { CodeMaker } from 'codemaker';
import { GoParameter } from '../types';
/**
 * Packages arguments such that they can be sent correctly to the jsii runtime
 * library.
 *
 * @returns the expression to use in place of the arguments for the jsii
 *          runtime library call.
 */
export declare function emitArguments(code: CodeMaker, parameters: readonly GoParameter[], returnVarName: string): string | undefined;
//# sourceMappingURL=emit-arguments.d.ts.map