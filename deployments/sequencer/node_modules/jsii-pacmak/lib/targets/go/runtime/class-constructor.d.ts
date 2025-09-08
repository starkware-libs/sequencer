import { CodeMaker } from 'codemaker';
import { EmitContext } from '../emit-context';
import { GoClassConstructor } from '../types';
export declare class ClassConstructor {
    readonly parent: GoClassConstructor;
    constructor(parent: GoClassConstructor);
    emit({ code, runtimeTypeChecking }: EmitContext): void;
    emitOverride(code: CodeMaker, instanceVar: string): void;
}
//# sourceMappingURL=class-constructor.d.ts.map