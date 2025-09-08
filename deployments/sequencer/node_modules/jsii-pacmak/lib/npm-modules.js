"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.findJsiiModules = findJsiiModules;
exports.updateAllNpmIgnores = updateAllNpmIgnores;
const spec = require("@jsii/spec");
const fs = require("fs-extra");
const path = require("path");
const packaging_1 = require("./packaging");
const toposort_1 = require("./toposort");
const util_1 = require("./util");
const logging = require("../lib/logging");
/**
 * Find all modules that need to be packagerd
 *
 * If the input list is empty, include the current directory.
 *
 * The result is topologically sorted.
 */
async function findJsiiModules(directories, recurse) {
    const ret = [];
    const visited = new Set();
    const toVisit = directories.length > 0 ? directories : ['.'];
    await Promise.all(toVisit.map((dir) => visitPackage(dir, true)));
    return (0, toposort_1.topologicalSort)(ret, (m) => m.name, (m) => m.dependencyNames);
    async function visitPackage(dir, isRoot) {
        const realPath = await fs.realpath(dir);
        if (visited.has(realPath)) {
            return;
        } // Already visited
        visited.add(realPath);
        const pkg = await fs.readJson(path.join(realPath, 'package.json'));
        if (!pkg.jsii?.outdir || !pkg.jsii?.targets) {
            if (isRoot) {
                throw new Error(`Invalid "jsii" section in ${realPath}. Expecting "outdir" and "targets"`);
            }
            else {
                return; // just move on, this is not a jsii package
            }
        }
        if (!pkg.name) {
            throw new Error(`package.json does not have a 'name' field: ${JSON.stringify(pkg, undefined, 2)}`);
        }
        const dependencyNames = [
            ...Object.keys(pkg.dependencies ?? {}),
            ...Object.keys(pkg.peerDependencies ?? {}),
            ...Object.keys(pkg.devDependencies ?? {}),
        ];
        // if --recurse is set, find dependency dirs and build them.
        if (recurse) {
            await Promise.all(dependencyNames.flatMap(async (dep) => {
                if ((0, util_1.isBuiltinModule)(dep)) {
                    return [];
                }
                try {
                    const depDir = await (0, util_1.findDependencyDirectory)(dep, realPath);
                    return [await visitPackage(depDir, false)];
                }
                catch (e) {
                    // Some modules like `@types/node` cannot be require()d, but we also don't need them.
                    if (!['MODULE_NOT_FOUND', 'ERR_PACKAGE_PATH_NOT_EXPORTED'].includes(e.code)) {
                        throw e;
                    }
                    return [];
                }
            }));
        }
        // outdir is either by package.json/jsii.outdir (relative to package root) or via command line (relative to cwd)
        const outputDirectory = pkg.jsii.outdir && path.resolve(realPath, pkg.jsii.outdir);
        const targets = [...Object.keys(pkg.jsii.targets), 'js']; // "js" is an implicit target.
        ret.push(new packaging_1.JsiiModule({
            name: pkg.name,
            moduleDirectory: realPath,
            defaultOutputDirectory: outputDirectory,
            availableTargets: targets,
            dependencyNames,
        }));
    }
}
async function updateAllNpmIgnores(packages) {
    await Promise.all(packages.map((pkg) => updateNpmIgnore(pkg.moduleDirectory, pkg.outputDirectory)));
}
async function updateNpmIgnore(packageDir, excludeOutdir) {
    const npmIgnorePath = path.join(packageDir, '.npmignore');
    let lines = new Array();
    let modified = false;
    if (await fs.pathExists(npmIgnorePath)) {
        lines = (await fs.readFile(npmIgnorePath)).toString().split('\n');
    }
    // if this is a fresh .npmignore, we can be a bit more opinionated
    // otherwise, we add just add stuff that's critical
    if (lines.length === 0) {
        excludePattern('Exclude typescript source and config', '*.ts', 'tsconfig.json', '*.tsbuildinfo');
        includePattern('Include javascript files and typescript declarations', '*.js', '*.d.ts');
    }
    if (excludeOutdir) {
        excludePattern('Exclude jsii outdir', path.relative(packageDir, excludeOutdir));
    }
    includePattern('Include .jsii and .jsii.gz', spec.SPEC_FILE_NAME, spec.SPEC_FILE_NAME_COMPRESSED);
    if (modified) {
        await fs.writeFile(npmIgnorePath, `${lines.join('\n')}\n`);
        logging.info('Updated .npmignore');
    }
    function includePattern(comment, ...patterns) {
        excludePattern(comment, ...patterns.map((p) => `!${p}`));
    }
    function excludePattern(comment, ...patterns) {
        let first = true;
        for (const pattern of patterns) {
            if (lines.includes(pattern)) {
                return; // already in .npmignore
            }
            modified = true;
            if (first) {
                lines.push('');
                lines.push(`# ${comment}`);
                first = false;
            }
            lines.push(pattern);
        }
    }
}
//# sourceMappingURL=npm-modules.js.map