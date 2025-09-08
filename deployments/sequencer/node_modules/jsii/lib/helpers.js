"use strict";
/**
 * Helper routines for use with the jsii compiler
 *
 * These are mostly used for testing, but all projects that need to exercise
 * the JSII compiler to test something need to share this code, so might as
 * well put it in one reusable place.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.TestWorkspace = void 0;
exports.sourceToAssemblyHelper = sourceToAssemblyHelper;
exports.compileJsiiForTest = compileJsiiForTest;
exports.normalizeConfigPath = normalizeConfigPath;
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const spec_1 = require("@jsii/spec");
const typescript_1 = require("typescript");
const compiler_1 = require("./compiler");
const project_info_1 = require("./project-info");
const utils_1 = require("./utils");
/**
 * Compile a piece of source and return the JSII assembly for it
 *
 * Only usable for trivial cases and tests.
 *
 * @param source can either be a single `string` (the content of `index.ts`), or
 *               a map of fileName to content, which *must* include `index.ts`.
 * @param options accepts a callback for historical reasons but really expects to
 *                take an options object.
 */
function sourceToAssemblyHelper(source, options) {
    return compileJsiiForTest(source, options).assembly;
}
/**
 * Compile a piece of source and return the assembly and compiled sources for it
 *
 * Only usable for trivial cases and tests.
 *
 * @param source can either be a single `string` (the content of `index.ts`), or
 *               a map of fileName to content, which *must* include `index.ts`.
 * @param options accepts a callback for historical reasons but really expects to
 *                take an options object.
 */
function compileJsiiForTest(source, options, compilerOptions) {
    if (typeof source === 'string') {
        source = { 'index.ts': source };
    }
    const inSomeLocation = isOptionsObject(options) && options.compilationDirectory ? inOtherDir(options.compilationDirectory) : inTempDir;
    // Easiest way to get the source into the compiler is to write it to disk somewhere.
    // I guess we could make an in-memory compiler host but that seems like work...
    return inSomeLocation(() => {
        for (const [fileName, content] of Object.entries(source)) {
            fs.mkdirSync(path.dirname(fileName), { recursive: true });
            fs.writeFileSync(fileName, content, { encoding: 'utf-8' });
        }
        const { projectInfo, packageJson } = makeProjectInfo('index.ts', typeof options === 'function'
            ? options
            : (pi) => {
                Object.assign(pi, options?.packageJson ?? options?.projectInfo ?? {});
            });
        const compiler = new compiler_1.Compiler({
            projectInfo,
            ...compilerOptions,
        });
        const emitResult = compiler.emit();
        const errors = emitResult.diagnostics.filter((d) => d.category === typescript_1.DiagnosticCategory.Error);
        for (const error of errors) {
            console.error((0, utils_1.formatDiagnostic)(error, projectInfo.projectRoot));
            // logDiagnostic() doesn't work out of the box, so console.error() it is.
        }
        if (errors.length > 0 || emitResult.emitSkipped) {
            throw new utils_1.JsiiError('There were compiler errors');
        }
        const assembly = (0, spec_1.loadAssemblyFromPath)(process.cwd(), false);
        const files = {};
        for (const filename of Object.keys(source)) {
            let jsFile = filename.replace(/\.ts$/, '.js');
            let dtsFile = filename.replace(/\.ts$/, '.d.ts');
            if (projectInfo.tsc?.outDir && filename !== 'README.md') {
                jsFile = path.join(projectInfo.tsc.outDir, jsFile);
                dtsFile = path.join(projectInfo.tsc.outDir, dtsFile);
            }
            // eslint-disable-next-line no-await-in-loop
            files[jsFile] = fs.readFileSync(jsFile, { encoding: 'utf-8' });
            // eslint-disable-next-line no-await-in-loop
            files[dtsFile] = fs.readFileSync(dtsFile, { encoding: 'utf-8' });
            const warningsFileName = '.warnings.jsii.js';
            if (fs.existsSync(warningsFileName)) {
                // eslint-disable-next-line no-await-in-loop
                files[warningsFileName] = fs.readFileSync(warningsFileName, {
                    encoding: 'utf-8',
                });
            }
        }
        return {
            assembly,
            files,
            packageJson,
            compressAssembly: isOptionsObject(options) && options.compressAssembly ? true : false,
        };
    });
}
function inTempDir(block) {
    const origDir = process.cwd();
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'jsii'));
    process.chdir(tmpDir);
    const ret = block();
    process.chdir(origDir);
    fs.rmSync(tmpDir, { force: true, recursive: true });
    return ret;
}
function inOtherDir(dir) {
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-constraint
    return (block) => {
        const origDir = process.cwd();
        process.chdir(dir);
        try {
            return block();
        }
        finally {
            process.chdir(origDir);
        }
    };
}
/**
 * Obtain project info so we can call the compiler
 *
 * Creating this directly in-memory leads to slightly different behavior from calling
 * jsii from the command-line, and I don't want to figure out right now.
 *
 * Most consistent behavior seems to be to write a package.json to disk and
 * then calling the same functions as the CLI would.
 */
function makeProjectInfo(types, cb) {
    const packageJson = {
        types,
        main: types.replace(/(?:\.d)?\.ts(x?)/, '.js$1'),
        name: 'testpkg', // That's what package.json would tell if we look up...
        version: '0.0.1',
        license: 'Apache-2.0',
        author: { name: 'John Doe' },
        repository: { type: 'git', url: 'https://github.com/aws/jsii.git' },
        jsii: {},
    };
    if (cb) {
        cb(packageJson);
    }
    fs.writeFileSync('package.json', JSON.stringify(packageJson, (_, v) => v, 2), 'utf-8');
    const { projectInfo } = (0, project_info_1.loadProjectInfo)(path.resolve(process.cwd(), '.'));
    return { projectInfo, packageJson };
}
function isOptionsObject(x) {
    return x ? typeof x === 'object' : false;
}
/**
 * An NPM-ready workspace where we can install test-compile dependencies and compile new assemblies
 */
class TestWorkspace {
    /**
     * Create a new workspace.
     *
     * Creates a temporary directory, don't forget to call cleanUp
     */
    static create() {
        const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'jsii-testworkspace'));
        fs.mkdirSync(tmpDir, { recursive: true });
        return new TestWorkspace(tmpDir);
    }
    /**
     * Execute a block with a temporary workspace
     */
    static withWorkspace(block) {
        const ws = TestWorkspace.create();
        try {
            return block(ws);
        }
        finally {
            ws.cleanup();
        }
    }
    constructor(rootDirectory) {
        this.rootDirectory = rootDirectory;
        this.installed = new Set();
    }
    /**
     * Add a test-compiled jsii assembly as a dependency
     */
    addDependency(dependencyAssembly) {
        if (this.installed.has(dependencyAssembly.assembly.name)) {
            throw new utils_1.JsiiError(`A dependency with name '${dependencyAssembly.assembly.name}' was already installed. Give one a different name.`);
        }
        this.installed.add(dependencyAssembly.assembly.name);
        // The following is silly, however: the helper has compiled the given source to
        // an assembly, and output files, and then removed their traces from disk.
        // We need those files back on disk, so write them back out again.
        //
        // We will drop them in 'node_modules/<name>' so they can be imported
        // as if they were installed.
        const modDir = path.join(this.rootDirectory, 'node_modules', dependencyAssembly.assembly.name);
        fs.mkdirSync(modDir, { recursive: true });
        (0, spec_1.writeAssembly)(modDir, dependencyAssembly.assembly, {
            compress: dependencyAssembly.compressAssembly,
        });
        fs.writeFileSync(path.join(modDir, 'package.json'), JSON.stringify(dependencyAssembly.packageJson, null, 2), 'utf-8');
        for (const [fileName, fileContents] of Object.entries(dependencyAssembly.files)) {
            fs.mkdirSync(path.dirname(path.join(modDir, fileName)), {
                recursive: true,
            });
            fs.writeFileSync(path.join(modDir, fileName), fileContents);
        }
    }
    dependencyDir(name) {
        if (!this.installed.has(name)) {
            throw new utils_1.JsiiError(`No dependency with name '${name}' has been installed`);
        }
        return path.join(this.rootDirectory, 'node_modules', name);
    }
    cleanup() {
        fs.rmSync(this.rootDirectory, { force: true, recursive: true });
    }
}
exports.TestWorkspace = TestWorkspace;
/**
 * TSConfig paths can either be relative to the project or absolute.
 * This function normalizes paths to be relative to the provided root.
 * After normalization, code using these paths can be much simpler.
 *
 * @param root the project root
 * @param pathToNormalize the path to normalize, might be empty
 */
function normalizeConfigPath(root, pathToNormalize) {
    if (pathToNormalize == null || !path.isAbsolute(pathToNormalize)) {
        return pathToNormalize;
    }
    return path.relative(root, pathToNormalize);
}
//# sourceMappingURL=helpers.js.map