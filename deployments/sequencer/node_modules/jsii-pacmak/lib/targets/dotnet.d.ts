import * as spec from '@jsii/spec';
import { TargetBuilder, BuildOptions } from '../builder';
import { JsiiModule } from '../packaging';
import { PackageInfo, Target, TargetOptions } from '../target';
import { DotNetGenerator } from './dotnet/dotnetgenerator';
export declare const TARGET_FRAMEWORK = "netcoreapp3.1";
/**
 * Build .NET packages all together, by generating an aggregate solution file
 */
export declare class DotnetBuilder implements TargetBuilder {
    private readonly modules;
    private readonly options;
    private readonly targetName;
    constructor(modules: readonly JsiiModule[], options: BuildOptions);
    buildModules(): Promise<void>;
    private generateAggregateSourceDir;
    private copyOutArtifacts;
    private generateModuleCode;
    /**
     * Decide whether or not to append 'dotnet' to the given output directory
     */
    private outputDir;
    /**
     * Write a NuGet.config that will include build directories for local packages not in the current build
     *
     */
    private generateNuGetConfigForLocalDeps;
    private makeTarget;
}
export default class Dotnet extends Target {
    static toPackageInfos(assm: spec.Assembly): {
        [language: string]: PackageInfo;
    };
    static toNativeReference(_type: spec.Type, options: any): {
        'c#': string;
    };
    protected readonly generator: DotNetGenerator;
    constructor(options: TargetOptions, assembliesCurrentlyBeingCompiled: string[]);
    build(_sourceDir: string, _outDir: string): Promise<void>;
}
//# sourceMappingURL=dotnet.d.ts.map