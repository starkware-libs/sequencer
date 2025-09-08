import { Assembly } from '@jsii/spec';
import { CodeMaker } from 'codemaker';
export declare class DotNetDependency {
    readonly namespace: string;
    readonly packageId: string;
    readonly fqn: string;
    readonly partOfCompilation: boolean;
    readonly version: string;
    constructor(namespace: string, packageId: string, fqn: string, version: string, partOfCompilation: boolean);
}
export declare class FileGenerator {
    private readonly assm;
    private readonly tarballFileName;
    private readonly code;
    private readonly assemblyInfoNamespaces;
    private readonly nameutils;
    constructor(assm: Assembly, tarballFileName: string, code: CodeMaker);
    generateProjectFile(dependencies: Map<string, DotNetDependency>, iconFile?: string): void;
    generateAssemblyInfoFile(): void;
    private getDescription;
    private getDecoratedVersion;
}
//# sourceMappingURL=filegenerator.d.ts.map