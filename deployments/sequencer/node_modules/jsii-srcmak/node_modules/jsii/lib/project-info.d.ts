import * as spec from '@jsii/spec';
import * as ts from 'typescript';
import { TypeScriptConfigValidationRuleSet } from './tsconfig';
export type TSCompilerOptions = Partial<Pick<ts.CompilerOptions, 'outDir' | 'rootDir' | 'baseUrl' | 'paths' | 'forceConsistentCasingInFileNames' | 'noImplicitOverride' | 'noPropertyAccessFromIndexSignature' | 'noUncheckedIndexedAccess' | 'declarationMap' | 'inlineSourceMap' | 'inlineSources' | 'sourceMap' | 'types'>>;
export interface ProjectInfo {
    readonly projectRoot: string;
    readonly packageJson: PackageJson;
    readonly name: string;
    readonly version: string;
    readonly author: spec.Person;
    readonly deprecated?: string;
    readonly stability?: spec.Stability;
    readonly license: string;
    readonly repository: {
        readonly type: string;
        readonly url: string;
        readonly directory?: string;
    };
    readonly keywords?: readonly string[];
    readonly main: string;
    readonly types: string;
    readonly dependencies: {
        readonly [name: string]: string;
    };
    readonly peerDependencies: {
        readonly [name: string]: string;
    };
    readonly dependencyClosure: readonly spec.Assembly[];
    readonly bundleDependencies?: {
        readonly [name: string]: string;
    };
    readonly targets: spec.AssemblyTargets;
    readonly metadata?: {
        readonly [key: string]: any;
    };
    readonly jsiiVersionFormat: 'short' | 'full';
    readonly diagnostics?: {
        readonly [code: string]: ts.DiagnosticCategory;
    };
    readonly description?: string;
    readonly homepage?: string;
    readonly contributors?: readonly spec.Person[];
    readonly excludeTypescript: readonly string[];
    readonly projectReferences?: boolean;
    readonly tsc?: TSCompilerOptions;
    readonly bin?: {
        readonly [name: string]: string;
    };
    readonly exports?: {
        readonly [name: string]: string | {
            readonly [name: string]: string;
        };
    };
    readonly tsconfig?: string;
    readonly validateTsconfig?: TypeScriptConfigValidationRuleSet;
}
export interface PackageJson {
    readonly description?: string;
    readonly homepage?: string;
    readonly name?: string;
    readonly version?: string;
    readonly keywords?: readonly string[];
    readonly license?: string;
    readonly private?: boolean;
    readonly exports?: {
        readonly [path: string]: string | {
            readonly [name: string]: string;
        };
    };
    readonly main?: string;
    readonly types?: string;
    /**
     * @example { "<4.0": { "*": ["ts3.9/*"] } }
     * @example { "<4.0": { "index.d.ts": ["index.ts3-9.d.ts"] } }
     */
    readonly typesVersions?: {
        readonly [versionRange: string]: {
            readonly [pattern: string]: readonly string[];
        };
    };
    readonly bin?: {
        readonly [name: string]: string;
    };
    readonly stability?: string;
    readonly deprecated?: string;
    readonly dependencies?: {
        readonly [name: string]: string;
    };
    readonly devDependencies?: {
        readonly [name: string]: string;
    };
    readonly peerDependencies?: {
        readonly [name: string]: string;
    };
    readonly bundleDependencies?: readonly string[];
    readonly bundledDependencies?: readonly string[];
    readonly jsii?: {
        readonly diagnostics?: {
            readonly [id: string]: 'error' | 'warning' | 'suggestion' | 'message';
        };
        readonly metadata?: {
            readonly [key: string]: unknown;
        };
        readonly targets?: {
            readonly [name: string]: unknown;
        };
        readonly versionFormat?: 'short' | 'full';
        readonly tsconfig?: string;
        readonly validateTsconfig?: string;
        readonly excludeTypescript?: readonly string[];
        readonly projectReferences?: boolean;
        readonly tsc?: TSCompilerOptions;
        readonly [key: string]: unknown;
    };
    readonly [key: string]: unknown;
}
export interface ProjectInfoResult {
    readonly projectInfo: ProjectInfo;
    readonly diagnostics: readonly ts.Diagnostic[];
}
export declare function loadProjectInfo(projectRoot: string): ProjectInfoResult;
//# sourceMappingURL=project-info.d.ts.map