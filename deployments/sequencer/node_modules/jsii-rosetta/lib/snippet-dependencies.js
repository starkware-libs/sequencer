"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.collectDependencies = collectDependencies;
exports.expandWithTransitiveDependencies = expandWithTransitiveDependencies;
exports.resolveDependenciesFromPackageJson = resolveDependenciesFromPackageJson;
exports.validateAvailableDependencies = validateAvailableDependencies;
exports.prepareDependencyDirectory = prepareDependencyDirectory;
const cp = require("node:child_process");
const node_fs_1 = require("node:fs");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const fastGlob = require("fast-glob");
const semver = require("semver");
const find_utils_1 = require("./find-utils");
const logging = require("./logging");
const util_1 = require("./util");
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { intersect } = require('semver-intersect');
/**
 * Collect the dependencies of a bunch of snippets together in one declaration
 *
 * We assume here the dependencies will not conflict.
 */
function collectDependencies(snippets) {
    const ret = {};
    for (const snippet of snippets) {
        for (const [name, source] of Object.entries(snippet.compilationDependencies ?? {})) {
            ret[name] = resolveConflict(name, source, ret[name]);
        }
    }
    return ret;
}
/**
 * Add transitive dependencies of concrete dependencies to the array
 *
 * This is necessary to prevent multiple copies of transitive dependencies on disk, which
 * jsii-based packages might not deal with very well.
 */
async function expandWithTransitiveDependencies(deps) {
    const pathsSeen = new Set();
    const queue = Object.values(deps).filter(isConcrete);
    let next = queue.shift();
    while (next) {
        await addDependenciesOf(next.resolvedDirectory);
        next = queue.shift();
    }
    async function addDependenciesOf(dir) {
        if (pathsSeen.has(dir)) {
            return;
        }
        pathsSeen.add(dir);
        try {
            const pj = JSON.parse(await node_fs_1.promises.readFile(path.join(dir, 'package.json'), { encoding: 'utf-8' }));
            for (const [name, dep] of Object.entries(await resolveDependenciesFromPackageJson(pj, dir))) {
                if (!deps[name]) {
                    deps[name] = dep;
                    queue.push(dep);
                }
            }
        }
        catch (e) {
            if (e.code === 'ENOENT') {
                return;
            }
            throw e;
        }
    }
}
/**
 * Find the corresponding package directories for all dependencies in a package.json
 */
async function resolveDependenciesFromPackageJson(packageJson, directory) {
    return (0, util_1.mkDict)(await Promise.all(Object.keys({ ...packageJson?.dependencies, ...packageJson?.peerDependencies })
        .filter((name) => !(0, find_utils_1.isBuiltinModule)(name))
        .filter((name) => !packageJson?.bundledDependencies?.includes(name) && !packageJson?.bundleDependencies?.includes(name))
        .map(async (name) => [
        name,
        {
            type: 'concrete',
            resolvedDirectory: await node_fs_1.promises.realpath(await (0, find_utils_1.findDependencyDirectory)(name, directory)),
        },
    ])));
}
function resolveConflict(name, a, b) {
    if (!b) {
        return a;
    }
    if (a.type === 'concrete' && b.type === 'concrete') {
        if (b.resolvedDirectory !== a.resolvedDirectory) {
            throw new Error(`Dependency conflict: ${name} can be either ${a.resolvedDirectory} or ${b.resolvedDirectory}`);
        }
        return a;
    }
    if (a.type === 'symbolic' && b.type === 'symbolic') {
        // Intersect the ranges
        return {
            type: 'symbolic',
            versionRange: myVersionIntersect(a.versionRange, b.versionRange),
        };
    }
    if (a.type === 'concrete' && b.type === 'symbolic') {
        const concreteVersion = JSON.parse(fs.readFileSync(path.join(a.resolvedDirectory, 'package.json'), 'utf-8')).version;
        if (!semver.satisfies(concreteVersion, b.versionRange, { includePrerelease: true })) {
            throw new Error(`Dependency conflict: ${name} expected to match ${b.versionRange} but found ${concreteVersion} at ${a.resolvedDirectory}`);
        }
        return a;
    }
    if (a.type === 'symbolic' && b.type === 'concrete') {
        // Reverse roles so we fall into the previous case
        return resolveConflict(name, b, a);
    }
    throw new Error('Cases should have been exhaustive');
}
/**
 * Check that the directory we were given has all the necessary dependencies in it
 *
 * It's a warning if this is not true, not an error.
 */
async function validateAvailableDependencies(directory, deps) {
    logging.info(`Validating dependencies at ${directory}`);
    const failures = await Promise.all(Object.entries(deps).flatMap(async ([name, _dep]) => {
        try {
            await (0, find_utils_1.findDependencyDirectory)(name, directory);
            return [];
        }
        catch {
            return [name];
        }
    }));
    if (failures.length > 0) {
        logging.warn(`${directory}: packages necessary to compile examples missing from supplied directory: ${failures.join(', ')}`);
    }
}
/**
 * Intersect two semver ranges
 *
 * The package we are using for this doesn't support all syntaxes yet.
 * Do some work on top.
 */
function myVersionIntersect(a, b) {
    if (a === '*') {
        return b;
    }
    if (b === '*') {
        return a;
    }
    try {
        return intersect(a, b);
    }
    catch (e) {
        throw new Error(`semver-intersect does not support either '${a}' or '${b}': ${e.message}`);
    }
}
/**
 * Prepare a temporary directory with symlinks to all the dependencies we need.
 *
 * - Symlinks the concrete dependencies
 * - Tries to first find the symbolic dependencies in a potential monorepo that might be present
 *   (try both `lerna` and `yarn` monorepos).
 * - Installs the remaining symbolic dependencies using 'npm'.
 */
async function prepareDependencyDirectory(deps) {
    const concreteDirs = Object.values(deps)
        .filter(isConcrete)
        .map((x) => x.resolvedDirectory);
    const monorepoPackages = await scanMonoRepos(concreteDirs);
    const tmpDir = await node_fs_1.promises.mkdtemp(path.join(os.tmpdir(), 'rosetta'));
    logging.info(`Preparing dependency closure at ${tmpDir} (-vv for more details)`);
    // Resolved symbolic packages against monorepo
    const resolvedDeps = (0, util_1.mkDict)(Object.entries(deps).map(([name, dep]) => [
        name,
        dep.type === 'concrete'
            ? dep
            : (monorepoPackages[name]
                ? { type: 'concrete', resolvedDirectory: monorepoPackages[name] }
                : dep),
    ]));
    const dependencies = {};
    for (const [name, dep] of Object.entries(resolvedDeps)) {
        if (isConcrete(dep)) {
            logging.debug(`${name} -> ${dep.resolvedDirectory}`);
            dependencies[name] = `file:${dep.resolvedDirectory}`;
        }
        else {
            logging.debug(`${name} @ ${dep.versionRange}`);
            dependencies[name] = dep.versionRange;
        }
    }
    await node_fs_1.promises.writeFile(path.join(tmpDir, 'package.json'), JSON.stringify({
        name: 'examples',
        version: '0.0.1',
        private: true,
        dependencies,
    }, undefined, 2), {
        encoding: 'utf-8',
    });
    // Run NPM install on this package.json.
    cp.execSync([
        'npm install',
        // We need to include --force for packages
        // that have a symbolic version in the symlinked dev tree (like "0.0.0"), but have
        // actual version range dependencies from externally installed packages (like "^2.0.0").
        '--force',
        // this is critical from a security perspective to prevent
        // code execution as part of the install command using npm hooks. (e.g postInstall)
        '--ignore-scripts',
        // save time by not running audit
        '--no-audit',
        // ensures npm does not insert anything in $PATH
        '--no-bin-links',
        // don't write or update a package-lock.json file
        '--no-package-lock',
        // only print errors
        `--loglevel error`,
    ].join(' '), {
        cwd: tmpDir,
        encoding: 'utf-8',
    });
    return tmpDir;
}
/**
 * Map package name to directory
 */
async function scanMonoRepos(startingDirs) {
    const globs = new Set();
    for (const dir of startingDirs) {
        // eslint-disable-next-line no-await-in-loop
        setExtend(globs, await findMonoRepoGlobs(dir));
    }
    if (globs.size === 0) {
        return {};
    }
    logging.debug(`Monorepo package sources: ${Array.from(globs).join(', ')}`);
    const packageDirectories = await fastGlob(Array.from(globs).map(windowsToUnix), { onlyDirectories: true });
    const results = (0, util_1.mkDict)((await Promise.all(packageDirectories.map(async (directory) => {
        const pjLocation = path.join(directory, 'package.json');
        return (await (0, util_1.pathExists)(pjLocation))
            ? [[JSON.parse(await node_fs_1.promises.readFile(pjLocation, 'utf-8')).name, directory]]
            : [];
    }))).flat());
    logging.debug(`Found ${Object.keys(results).length} packages in monorepo: ${(0, util_1.formatList)(Object.keys(results))}`);
    return results;
}
async function findMonoRepoGlobs(startingDir) {
    const ret = new Set();
    // Lerna monorepo
    const lernaJsonDir = await (0, find_utils_1.findUp)(startingDir, async (dir) => (0, util_1.pathExists)(path.join(dir, 'lerna.json')));
    if (lernaJsonDir) {
        const lernaJson = JSON.parse(await node_fs_1.promises.readFile(path.join(lernaJsonDir, 'lerna.json'), 'utf-8'));
        for (const glob of lernaJson?.packages ?? []) {
            ret.add(path.join(lernaJsonDir, glob));
        }
    }
    // Yarn monorepo
    const yarnWsDir = await (0, find_utils_1.findUp)(startingDir, async (dir) => (await (0, util_1.pathExists)(path.join(dir, 'package.json'))) &&
        JSON.parse(await node_fs_1.promises.readFile(path.join(dir, 'package.json'), 'utf-8'))?.workspaces !== undefined);
    if (yarnWsDir) {
        const yarnWs = JSON.parse(await node_fs_1.promises.readFile(path.join(yarnWsDir, 'package.json'), 'utf-8'));
        for (const glob of yarnWs.workspaces?.packages ?? []) {
            ret.add(path.join(yarnWsDir, glob));
        }
    }
    return ret;
}
function isConcrete(x) {
    return x.type === 'concrete';
}
function setExtend(xs, ys) {
    for (const y of ys) {
        xs.add(y);
    }
    return xs;
}
/**
 * Necessary for fastGlob
 */
function windowsToUnix(x) {
    return x.replace(/\\/g, '/');
}
//# sourceMappingURL=snippet-dependencies.js.map