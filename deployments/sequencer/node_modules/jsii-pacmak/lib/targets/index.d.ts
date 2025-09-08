import { TargetBuilder, BuildOptions } from '../builder';
import { JsiiModule } from '../packaging';
import { Toposorted } from '../toposort';
export declare enum TargetName {
    DOTNET = "dotnet",
    GO = "go",
    JAVA = "java",
    JAVASCRIPT = "js",
    PYTHON = "python"
}
export type BuilderFactory = (modules: Toposorted<JsiiModule>, options: BuildOptions) => TargetBuilder;
export declare const ALL_BUILDERS: {
    [key in TargetName]: BuilderFactory;
};
export declare const INCOMPLETE_DISCLAIMER_NONCOMPILING = "Example automatically generated from non-compiling source. May contain errors.";
//# sourceMappingURL=index.d.ts.map