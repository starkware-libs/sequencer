import type { Assembly } from '@jsii/spec';
import * as ts from 'typescript';
/**
 * Additional options that may be provided to the symbolIdentifier.
 */
interface SymbolIdOptions {
    /**
     * The assembly that the symbol is found in.
     * This is used to provide the correct root directory
     * as specified in the assembly metadata. In turn,
     * the root directory is used to ensure that the
     * symbolId comes from source code and not compiled code.
     */
    readonly assembly?: Assembly;
}
/**
 * Return a symbol identifier for the given symbol
 *
 * The symbol identifier identifies a TypeScript symbol in a source file inside
 * a package. We can use this to map between jsii entries in the manifest, and
 * entities in the TypeScript source code.
 *
 * Going via symbol id is the only way to identify symbols in submodules. Otherwise,
 * all the TypeScript compiler sees is:
 *
 * ```
 * /my/package/lib/source/directory/dist.js <containing> MyClass
 * ```
 *
 * And there's no way to figure out what submodule name
 * `lib/source/directory/dist` is exported as.
 *
 * The format of a symbol id is:
 *
 * ```
 * relative/source/file:Name.space.Class[#member]
 * ```
 *
 * We used to build this identifier ourselves. Turns out there was a built-in
 * way to get pretty much the same, by calling `typeChecker.getFullyQualifiedName()`.
 * Whoops ^_^ (this historical accident is why the format is similar to but
 * different from what the TS checker returns).
 */
export declare function symbolIdentifier(typeChecker: ts.TypeChecker, sym: ts.Symbol | undefined, options?: SymbolIdOptions): string | undefined;
/**
 * Ensures that the sourcePath is pointing to the source code
 * and not compiled code. This can happen if the root directory
 * and/or out directory is set for the project. We check to see
 * if the out directory is present in the sourcePath, and if so,
 * we replace it with the root directory.
 */
export declare function normalizePath(sourcePath: string, rootDir?: string, outDir?: string): string;
export {};
//# sourceMappingURL=symbol-id.d.ts.map