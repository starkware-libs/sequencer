import * as spec from '@jsii/spec';
import * as ts from 'typescript';
import { Emitter } from './emitter';
import { JsiiDiagnostic } from './jsii-diagnostic';
import { ProjectInfo } from './project-info';
export declare class Validator implements Emitter {
    readonly projectInfo: ProjectInfo;
    readonly assembly: spec.Assembly;
    static VALIDATIONS: ValidationFunction[];
    constructor(projectInfo: ProjectInfo, assembly: spec.Assembly);
    emit(): ts.EmitResult;
}
export type DiagnosticEmitter = (diag: JsiiDiagnostic) => void;
export type ValidationFunction = (validator: Validator, assembly: spec.Assembly, diagnostic: DiagnosticEmitter) => void;
//# sourceMappingURL=validator.d.ts.map