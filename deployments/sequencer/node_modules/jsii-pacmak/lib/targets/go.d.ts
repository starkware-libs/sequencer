import { Assembly } from 'jsii-reflect';
import { RosettaTabletReader } from 'jsii-rosetta';
import { IGenerator, Legalese } from '../generator';
import { Target, TargetOptions } from '../target';
import { RootPackage } from './go/package';
export declare class Golang extends Target {
    private readonly goGenerator;
    constructor(options: TargetOptions);
    get generator(): GoGenerator;
    /**
     * Generates a publishable artifact in `outDir`.
     *
     * @param sourceDir the directory where the generated source is located.
     * @param outDir    the directory where the publishable artifact should be placed.
     */
    build(sourceDir: string, outDir: string): Promise<void>;
    /**
     * Creates a copy of the `go.mod` file called `local.go.mod` with added
     * `replace` directives for local mono-repo dependencies. This is required in
     * order to run `go fmt` and `go build`.
     *
     * @param pkgDir The directory which contains the generated go code
     */
    private writeLocalGoMod;
}
declare class GoGenerator implements IGenerator {
    private assembly;
    rootPackage: RootPackage;
    private readonly code;
    private readonly documenter;
    private readonly rosetta;
    private readonly runtimeTypeChecking;
    constructor(options: {
        readonly rosetta: RosettaTabletReader;
        readonly runtimeTypeChecking: boolean;
    });
    load(_: string, assembly: Assembly): Promise<void>;
    upToDate(_outDir: string): Promise<boolean>;
    generate(): void;
    save(outDir: string, tarball: string, { license, notice }: Legalese): Promise<any>;
}
export {};
//# sourceMappingURL=go.d.ts.map