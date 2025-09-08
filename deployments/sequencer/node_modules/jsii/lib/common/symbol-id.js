"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.symbolIdentifier = symbolIdentifier;
exports.normalizePath = normalizePath;
const fs = require("node:fs");
const path = require("node:path");
const ts = require("typescript");
const find_utils_1 = require("./find-utils");
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
function symbolIdentifier(typeChecker, sym, options = {}) {
    if (!sym) {
        return undefined;
    }
    // If this symbol happens to be an alias, resolve it first
    // eslint-disable-next-line no-bitwise
    while ((sym.flags & ts.SymbolFlags.Alias) !== 0) {
        sym = typeChecker.getAliasedSymbol(sym);
    }
    const isMember = 
    // eslint-disable-next-line no-bitwise
    (sym.flags &
        // eslint-disable-next-line no-bitwise
        (ts.SymbolFlags.Method |
            // eslint-disable-next-line no-bitwise
            ts.SymbolFlags.Property |
            ts.SymbolFlags.EnumMember)) !==
        0;
    const tsName = typeChecker.getFullyQualifiedName(sym);
    // TypeScript fqn looks like "/path/to/file"[.name.in.file]
    const groups = /^"([^"]+)"(?:\.(.*))?$/.exec(tsName);
    if (!groups) {
        return undefined;
    }
    const [, fileName, inFileName] = groups; // inFileName may be absent
    const relFile = Helper.for(typeChecker).assemblyRelativeSourceFile(fileName, options?.assembly);
    if (!relFile) {
        return undefined;
    }
    // If this is a member symbol, replace the final '.' with a '#'
    const typeSymbol = isMember ? (inFileName ?? '').replace(/\.([^.]+)$/, '#$1') : inFileName ?? '';
    return `${relFile}:${typeSymbol}`;
}
class Helper {
    static for(typeChecker) {
        const cached = this.INSTANCES.get(typeChecker);
        if (cached != null) {
            return cached;
        }
        const helper = new Helper();
        this.INSTANCES.set(typeChecker, helper);
        return helper;
    }
    constructor() {
        this.packageInfo = new Map();
    }
    assemblyRelativeSourceFile(sourceFileName, asm) {
        const packageInfo = this.findPackageInfo(path.dirname(sourceFileName));
        if (!packageInfo) {
            return undefined;
        }
        let sourcePath = removePrefix(packageInfo.outdir ?? '', path.relative(packageInfo.packageJsonDir, sourceFileName));
        // Modify the namespace if we send in the assembly.
        if (asm) {
            const tscRootDir = packageInfo.tscRootDir ?? asm.metadata?.tscRootDir;
            const tscOutDir = packageInfo.tscOutDir;
            sourcePath = normalizePath(sourcePath, tscRootDir, tscOutDir);
        }
        return sourcePath.replace(/(\.d)?\.ts$/, '');
        function removePrefix(prefix, filePath) {
            const prefixParts = prefix.split(/[/\\]/g);
            const pathParts = filePath.split(/[/\\]/g);
            let i = 0;
            while (prefixParts[i] === pathParts[i]) {
                i++;
            }
            return pathParts.slice(i).join('/');
        }
    }
    findPackageInfo(from) {
        if (this.packageInfo.has(from)) {
            return this.packageInfo.get(from);
        }
        const packageJsonDir = (0, find_utils_1.findUp)(from, (dir) => fs.existsSync(path.join(dir, 'package.json')));
        if (!packageJsonDir) {
            this.packageInfo.set(from, undefined);
            return undefined;
        }
        if (this.packageInfo.has(packageJsonDir)) {
            return this.packageInfo.get(packageJsonDir);
        }
        const { jsii } = JSON.parse(fs.readFileSync(path.join(packageJsonDir, 'package.json'), 'utf-8'));
        const result = {
            packageJsonDir,
            outdir: jsii?.outdir,
            tscRootDir: jsii?.tsc?.rootDir,
            tscOutDir: jsii?.tsc?.outDir,
        };
        this.packageInfo.set(from, result);
        this.packageInfo.set(packageJsonDir, result);
        return result;
    }
}
Helper.INSTANCES = new WeakMap();
/**
 * Ensures that the sourcePath is pointing to the source code
 * and not compiled code. This can happen if the root directory
 * and/or out directory is set for the project. We check to see
 * if the out directory is present in the sourcePath, and if so,
 * we replace it with the root directory.
 */
function normalizePath(sourcePath, rootDir, outDir) {
    if (rootDir === undefined || outDir === undefined) {
        return sourcePath;
    }
    outDir = removeEndSlash(path.normalize(outDir));
    const outDirLength = outDir.split(path.sep).length;
    rootDir = removeEndSlash(path.normalize(rootDir));
    let paths = path.normalize(sourcePath).split(path.sep);
    const pathDir = paths.slice(0, outDirLength).join(path.sep);
    if (outDir === pathDir || outDir === '.') {
        // outDir === '.' is a special case where we do not want
        // to remove any paths from the list.
        if (outDir !== '.') {
            paths = paths.slice(outDirLength);
        }
        sourcePath = rootDir === '.' ? paths.join('/') : `${rootDir}/${paths.join('/')}`;
    }
    return unixize(sourcePath);
    function removeEndSlash(filePath) {
        return filePath.endsWith(path.sep) ? filePath.slice(0, filePath.length - 1) : filePath;
    }
}
/**
 * Turn backslashes in a path into forward slashes
 */
function unixize(p) {
    return p.replace(/\\/g, '/');
}
//# sourceMappingURL=symbol-id.js.map