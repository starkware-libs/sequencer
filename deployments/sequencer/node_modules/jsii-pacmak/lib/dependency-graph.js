"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.traverseDependencyGraph = traverseDependencyGraph;
const fs = require("fs-extra");
const path_1 = require("path");
const util = require("./util");
/**
 * Traverses the dependency graph and invokes the provided callback method for
 * each individual dependency root directory (including the current package).
 * The dependency roots are de-duplicated based on their absolute path on the
 * file system.
 *
 * @param packageDir the current package's root directory (i.e: where the
 *                   `package.json` file is located)
 * @param callback   the function to invoke with each package's informations
 * @param host       the dependency graph traversal host to use (this parameter
 *                   should typically not be provided unless this module is
 *                   being unit tested)
 */
async function traverseDependencyGraph(packageDir, callback, host = {
    readJson: fs.readJson,
    findDependencyDirectory: util.findDependencyDirectory,
}) {
    return real$traverseDependencyGraph(packageDir, callback, host, new Set());
}
async function real$traverseDependencyGraph(packageDir, callback, host, visited) {
    // We're at the root if we have not visited anything yet. How convenient!
    const isRoot = visited.size === 0;
    if (visited.has(packageDir)) {
        return void 0;
    }
    visited.add(packageDir);
    const meta = await host.readJson((0, path_1.join)(packageDir, 'package.json'));
    if (!(await callback(packageDir, meta, isRoot))) {
        return void 0;
    }
    const deps = new Set([
        ...Object.keys(meta.dependencies ?? {}),
        ...Object.keys(meta.peerDependencies ?? {}),
    ]);
    return Promise.all(Array.from(deps)
        // No need to pacmak the dependency if it's built-in, or if it's bundled
        .filter((m) => !util.isBuiltinModule(m) &&
        !meta.bundledDependencies?.includes(m) &&
        !meta.bundleDependencies?.includes(m))
        .map(async (dep) => {
        const dependencyDir = await host.findDependencyDirectory(dep, packageDir);
        return real$traverseDependencyGraph(dependencyDir, callback, host, visited);
    })).then();
}
//# sourceMappingURL=dependency-graph.js.map