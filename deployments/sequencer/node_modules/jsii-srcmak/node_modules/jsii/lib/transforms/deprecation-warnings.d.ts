import { Assembly } from '@jsii/spec';
import * as ts from 'typescript';
import { ProjectInfo } from '../project-info';
export declare const WARNINGSCODE_FILE_NAME = ".warnings.jsii.js";
export declare class DeprecationWarningsInjector {
    private readonly typeChecker;
    private transformers;
    constructor(typeChecker: ts.TypeChecker);
    process(assembly: Assembly, projectInfo: ProjectInfo): void;
    get customTransformers(): ts.CustomTransformers;
    private buildTypeIndex;
}
//# sourceMappingURL=deprecation-warnings.d.ts.map