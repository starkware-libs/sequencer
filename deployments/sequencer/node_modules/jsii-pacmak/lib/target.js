"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Target = void 0;
exports.findLocalBuildDirs = findLocalBuildDirs;
const fs = require("fs-extra");
const path = require("path");
const spdx = require("spdx-license-list/full");
const dependency_graph_1 = require("./dependency-graph");
const logging = require("./logging");
class Target {
    constructor(options) {
        this.arguments = options.arguments;
        this.assembly = options.assembly;
        this.fingerprint = options.fingerprint ?? true;
        this.force = options.force ?? false;
        this.packageDir = options.packageDir;
        this.rosetta = options.rosetta;
        this.runtimeTypeChecking = options.runtimeTypeChecking;
        this.targetName = options.targetName;
    }
    /**
     * Emits code artifacts.
     *
     * @param outDir the directory where the generated source will be placed.
     */
    async generateCode(outDir, tarball) {
        await this.generator.load(this.packageDir, this.assembly);
        if (this.force || !(await this.generator.upToDate(outDir))) {
            this.generator.generate(this.fingerprint);
            const licenseFile = path.join(this.packageDir, 'LICENSE');
            const license = (await fs.pathExists(licenseFile))
                ? await fs.readFile(licenseFile, 'utf8')
                : spdx[this.assembly.license]?.licenseText;
            const noticeFile = path.join(this.packageDir, 'NOTICE');
            const notice = (await fs.pathExists(noticeFile))
                ? await fs.readFile(noticeFile, 'utf8')
                : undefined;
            await this.generator.save(outDir, tarball, { license, notice });
        }
        else {
            logging.info(`Generated code for ${this.targetName} was already up-to-date in ${outDir} (use --force to re-generate)`);
        }
    }
    /**
     * A utility to copy files from one directory to another.
     *
     * @param sourceDir the directory to copy from.
     * @param targetDir the directory to copy into.
     */
    async copyFiles(sourceDir, targetDir) {
        // Preemptively create target directory, to avoid unsafely racing on it's creation.
        await fs.mkdirp(targetDir);
        await fs.copy(sourceDir, targetDir, { recursive: true });
    }
    /**
     * Traverses the dep graph and returns a list of pacmak output directories
     * available locally for this specific target. This allows target builds to
     * take local dependencies in case a dependency is checked-out.
     *
     * @param packageDir The directory of the package to resolve from.
     */
    async findLocalDepsOutput(rootPackageDir) {
        return findLocalBuildDirs(rootPackageDir, this.targetName);
    }
}
exports.Target = Target;
/**
 * Traverses the dep graph and returns a list of pacmak output directories
 * available locally for this specific target. This allows target builds to
 * take local dependencies in case a dependency is checked-out.
 *
 * @param packageDir The directory of the package to resolve from.
 */
async function findLocalBuildDirs(rootPackageDir, targetName) {
    const results = new Set();
    await (0, dependency_graph_1.traverseDependencyGraph)(rootPackageDir, processPackage);
    return Array.from(results);
    async function processPackage(packageDir, pkg, isRoot) {
        // no jsii or jsii.outdir - either a misconfigured jsii package or a non-jsii dependency. either way, we are done here.
        if (!pkg.jsii || !pkg.jsii.outdir) {
            return false;
        }
        if (isRoot) {
            // This is the root package - no need to register it's outdir
            return true;
        }
        // if an output directory exists for this module, then we add it to our
        // list of results (unless it's the root package, which we are currently building)
        const outdir = path.join(packageDir, pkg.jsii.outdir, targetName);
        if (await fs.pathExists(outdir)) {
            logging.debug(`Found ${outdir} as a local dependency output`);
            results.add(outdir);
        }
        return true;
    }
}
//# sourceMappingURL=target.js.map