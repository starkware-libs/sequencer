import * as ts from 'typescript';
export declare class TypeScriptCompiler {
    private readonly realHost;
    /**
     * A compiler-scoped cache to avoid having to re-parse the same library files for every compilation
     */
    private readonly fileCache;
    createInMemoryCompilerHost(sourcePath: string, sourceContents: string, currentDirectory?: string): ts.CompilerHost;
    compileInMemory(filename: string, contents: string, currentDirectory?: string): CompilationResult;
}
export interface CompilationResult {
    program: ts.Program;
    rootFile: ts.SourceFile;
}
export declare const STANDARD_COMPILER_OPTIONS: ts.CompilerOptions;
//# sourceMappingURL=ts-compiler.d.ts.map