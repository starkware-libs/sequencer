"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Golang = void 0;
const codemaker_1 = require("codemaker");
const fs = require("fs-extra");
const path = require("path");
const logging = require("../logging");
const target_1 = require("../target");
const util_1 = require("../util");
const documentation_1 = require("./go/documentation");
const package_1 = require("./go/package");
const runtime_1 = require("./go/runtime");
const util_2 = require("./go/util");
class Golang extends target_1.Target {
    constructor(options) {
        super(options);
        this.goGenerator = new GoGenerator(options);
    }
    get generator() {
        return this.goGenerator;
    }
    /**
     * Generates a publishable artifact in `outDir`.
     *
     * @param sourceDir the directory where the generated source is located.
     * @param outDir    the directory where the publishable artifact should be placed.
     */
    async build(sourceDir, outDir) {
        // copy generated sources to the output directory
        await this.copyFiles(sourceDir, outDir);
        const pkgDir = path.join(outDir, this.goGenerator.rootPackage.packageName);
        // write `local.go.mod` with "replace" directives for local modules
        const localGoMod = await this.writeLocalGoMod(pkgDir);
        try {
            // run `go build` with local.go.mod, go 1.16+ requires that we download
            // modules explicit so go.sum is updated. We'd want to use
            // `go mod download`, but it does not add missing entries in the `go.sum`
            // file while `go mod tidy` does.
            await go('mod', ['tidy', '-modfile', localGoMod.path], {
                cwd: pkgDir,
            });
        }
        catch (e) {
            logging.info(`[${pkgDir}] Content of ${localGoMod.path} file:\n${localGoMod.content}`);
            // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
            return Promise.reject(e);
        }
        if (process.env.JSII_BUILD_GO) {
            // This step is taken to ensure that the generated code is compilable
            await go('build', ['-modfile', localGoMod.path, './...'], {
                cwd: pkgDir,
            });
        }
        // delete local.go.mod and local.go.sum from the output directory so it doesn't get published
        const localGoSum = `${path.basename(localGoMod.path, '.mod')}.sum`;
        await fs.remove(path.join(pkgDir, localGoMod.path));
        return fs.remove(path.join(pkgDir, localGoSum));
    }
    /**
     * Creates a copy of the `go.mod` file called `local.go.mod` with added
     * `replace` directives for local mono-repo dependencies. This is required in
     * order to run `go fmt` and `go build`.
     *
     * @param pkgDir The directory which contains the generated go code
     */
    async writeLocalGoMod(pkgDir) {
        const replace = {};
        // find local deps by check if `<jsii.outdir>/go` exists for dependencies
        // and also consider `outDir` in case pacmak is executed using `--outdir
        // --recurse` (in which case all go code will be generated there).
        const dirs = [
            path.dirname(pkgDir),
            ...(await (0, target_1.findLocalBuildDirs)(this.packageDir, 'go')),
        ];
        // try to resolve @jsii/go-runtime (only exists as a devDependency)
        const localModules = tryFindLocalRuntime();
        if (localModules != null) {
            for (const [name, localPath] of Object.entries(localModules)) {
                replace[name] = localPath;
            }
        }
        // iterate (recursively) on all package dependencies and check if we have a
        // local build directory for this module. if
        // we do, add a "replace" directive to point to it instead of download from
        // the network.
        const visit = async (pkg) => {
            for (const dep of pkg.packageDependencies) {
                for (const baseDir of dirs) {
                    // eslint-disable-next-line no-await-in-loop
                    const moduleDir = await tryFindLocalModule(baseDir, dep);
                    if (moduleDir) {
                        replace[dep.goModuleName] = moduleDir;
                        // we found a replacement for this dep, we can stop searching
                        break;
                    }
                }
                // recurse to transitive deps ("replace" is only considered at the top level go.mod)
                // eslint-disable-next-line no-await-in-loop
                await visit(dep);
            }
        };
        await visit(this.goGenerator.rootPackage);
        // write `local.go.mod`
        // read existing content
        const goMod = path.join(pkgDir, package_1.GOMOD_FILENAME);
        const lines = [await fs.readFile(goMod, 'utf-8'), '', '// Local packages:'];
        for (const [from, to] of Object.entries(replace)) {
            logging.info(`[${pkgDir}] Local replace: ${from} => ${to}`);
            lines.push(`replace ${from} => ${to}`);
        }
        const localGoMod = `local.${package_1.GOMOD_FILENAME}`;
        const content = lines.join('\n');
        await fs.writeFile(path.join(pkgDir, localGoMod), content, {
            encoding: 'utf-8',
        });
        return { path: localGoMod, content };
    }
}
exports.Golang = Golang;
class GoGenerator {
    constructor(options) {
        this.code = new codemaker_1.CodeMaker({
            indentCharacter: '\t',
            indentationLevel: 1,
        });
        this.rosetta = options.rosetta;
        this.documenter = new documentation_1.Documentation(this.code, this.rosetta);
        this.runtimeTypeChecking = options.runtimeTypeChecking;
    }
    async load(_, assembly) {
        this.assembly = assembly;
        return Promise.resolve();
    }
    async upToDate(_outDir) {
        return Promise.resolve(false);
    }
    generate() {
        this.rootPackage = new package_1.RootPackage(this.assembly);
        return this.rootPackage.emit({
            code: this.code,
            documenter: this.documenter,
            runtimeTypeChecking: this.runtimeTypeChecking,
        });
    }
    async save(outDir, tarball, { license, notice }) {
        const output = path.join(outDir, this.rootPackage.packageName);
        await this.code.save(output);
        await fs.copyFile(tarball, path.join(output, runtime_1.JSII_INIT_PACKAGE, (0, util_2.tarballName)(this.assembly)));
        if (license) {
            await fs.writeFile(path.join(output, 'LICENSE'), license, {
                encoding: 'utf8',
            });
        }
        if (notice) {
            await fs.writeFile(path.join(output, 'NOTICE'), notice, {
                encoding: 'utf8',
            });
        }
    }
}
/**
 * Checks if `buildDir` includes a local go build version (with "replace"
 * directives).
 * @param baseDir the `dist/go` directory
 * @returns `undefined` if not or the module directory otherwise.
 */
async function tryFindLocalModule(baseDir, pkg) {
    const gomodPath = path.join(baseDir, pkg.packageName, package_1.GOMOD_FILENAME);
    if (!(await fs.pathExists(gomodPath))) {
        return undefined;
    }
    // read `go.mod` and check that it is for the correct module
    const gomod = (await fs.readFile(gomodPath, 'utf-8')).split('\n');
    const isExpectedModule = gomod.find((line) => line.trim() === `module ${pkg.goModuleName}`);
    if (!isExpectedModule) {
        return undefined;
    }
    return path.resolve(path.dirname(gomodPath));
}
/**
 * Check if we are running from inside the jsii repository, and then we want to
 * use the local runtime instead of download from a released version.
 *
 * This is a generator that procudes an entry for each local module that
 * is identified under the local module path exposed by `@jsii/go-runtime` .
 */
function tryFindLocalRuntime() {
    try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires, @typescript-eslint/no-require-imports, import/no-extraneous-dependencies
        const localRuntime = require('@jsii/go-runtime');
        logging.debug(`Using @jsii/go-runtime from ${localRuntime.runtimePath}`);
        return localRuntime.runtimeModules;
    }
    catch {
        return undefined;
    }
}
/**
 * Executes a go CLI command.
 *
 *
 * @param command The `go` command to execute (e.g. `build`)
 * @param args Additional args
 * @param options Options
 */
async function go(command, args, options) {
    const { cwd } = options;
    return (0, util_1.shell)('go', [command, ...args], {
        cwd,
        env: {
            // disable the use of sumdb to reduce eventual consistency issues when new modules are published
            GOSUMDB: 'off',
        },
    });
}
//# sourceMappingURL=go.js.map