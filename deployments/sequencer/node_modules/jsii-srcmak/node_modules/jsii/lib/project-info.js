"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.loadProjectInfo = loadProjectInfo;
const fs = require("node:fs");
const path = require("node:path");
const spec = require("@jsii/spec");
const spec_1 = require("@jsii/spec");
const log4js = require("log4js");
const semver = require("semver");
const ts = require("typescript");
const find_utils_1 = require("./common/find-utils");
const jsii_diagnostic_1 = require("./jsii-diagnostic");
const tsconfig_1 = require("./tsconfig");
const utils_1 = require("./utils");
// eslint-disable-next-line @typescript-eslint/no-var-requires, @typescript-eslint/no-require-imports
const spdx = require('spdx-license-list/simple');
const LOG = log4js.getLogger('jsii/package-info');
function loadProjectInfo(projectRoot) {
    const packageJsonPath = path.join(projectRoot, 'package.json');
    const pkg = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
    const diagnostics = [];
    let bundleDependencies;
    for (const name of pkg.bundleDependencies ?? pkg.bundledDependencies ?? []) {
        const version = pkg.dependencies?.[name];
        if (!version) {
            throw new utils_1.JsiiError(`The "package.json" file has "${name}" in "bundleDependencies", but it is not declared in "dependencies"`);
        }
        if (pkg.peerDependencies && name in pkg.peerDependencies) {
            throw new utils_1.JsiiError(`The "package.json" file has "${name}" in "bundleDependencies", and also in "peerDependencies"`);
        }
        bundleDependencies = bundleDependencies ?? {};
        bundleDependencies[name] = _resolveVersion(version, projectRoot).version;
    }
    // Check peerDependencies are also in devDependencies
    // You need this to write tests properly. There are probably cases where
    // it makes sense to have this different, so most of what this checking
    // produces is warnings, not errors.
    const devDependencies = pkg.devDependencies ?? {};
    for (const [name, rng] of Object.entries(pkg.peerDependencies ?? {})) {
        const range = new semver.Range(_resolveVersion(rng, projectRoot).version);
        const minVersion = semver.minVersion(range)?.raw;
        if (!(name in devDependencies) || devDependencies[name] !== `${minVersion}`) {
            diagnostics.push(jsii_diagnostic_1.JsiiDiagnostic.JSII_0006_MISSING_DEV_DEPENDENCY.createDetached(name, `${rng}`, `${minVersion}`, `${devDependencies[name]}`));
            continue;
        }
    }
    const bundled = new Set(Object.keys(bundleDependencies ?? {}));
    const dependencies = filterDictByKey(pkg.dependencies ?? {}, (depName) => !bundled.has(depName));
    const peerDependencies = pkg.peerDependencies ?? {};
    const resolver = new DependencyResolver();
    const resolved = resolver.discoverDependencyTree(projectRoot, {
        ...dependencies,
        ...peerDependencies,
    });
    const transitiveDependencies = resolver.assemblyClosure(resolved);
    const metadata = mergeMetadata({
        jsii: {
            pacmak: {
                // When `true`, `jsii-pacmak` will use the `Jsii$Default` implementation in code generation even for dependencies.
                hasDefaultInterfaces: true,
            },
        },
    }, pkg.jsii?.metadata);
    const projectInfo = {
        projectRoot,
        packageJson: pkg,
        name: _required(pkg.name, 'The "package.json" file must specify the "name" attribute'),
        version: _required(pkg.version, 'The "package.json" file must specify the "version" attribute'),
        deprecated: pkg.deprecated,
        stability: _validateStability(pkg.stability, pkg.deprecated),
        author: _toPerson(_required(pkg.author, 'The "package.json" file must specify the "author" attribute'), 'author'),
        repository: _toRepository(_required(pkg.repository, 'The "package.json" file must specify the "repository" attribute')),
        license: _validateLicense(pkg.license),
        keywords: pkg.keywords,
        main: _required(pkg.main, 'The "package.json" file must specify the "main" attribute'),
        types: _required(pkg.types, 'The "package.json" file must specify the "types" attribute'),
        dependencies,
        peerDependencies,
        dependencyClosure: transitiveDependencies,
        bundleDependencies,
        targets: {
            ..._required(pkg.jsii, 'The "package.json" file must specify the "jsii" attribute').targets,
            js: { npm: pkg.name },
        },
        metadata,
        jsiiVersionFormat: _validateVersionFormat(pkg.jsii?.versionFormat ?? 'full'),
        description: pkg.description,
        homepage: pkg.homepage,
        contributors: pkg.contributors?.map((contrib, index) => _toPerson(contrib, `contributors[${index}]`, 'contributor')),
        excludeTypescript: pkg.jsii?.excludeTypescript ?? [],
        projectReferences: pkg.jsii?.projectReferences,
        tsc: {
            outDir: pkg.jsii?.tsc?.outDir,
            rootDir: pkg.jsii?.tsc?.rootDir,
            baseUrl: pkg.jsii?.tsc?.baseUrl,
            paths: pkg.jsii?.tsc?.paths,
            forceConsistentCasingInFileNames: pkg.jsii?.tsc?.forceConsistentCasingInFileNames,
            noImplicitOverride: pkg.jsii?.tsc?.noImplicitOverride,
            noPropertyAccessFromIndexSignature: pkg.jsii?.tsc?.noPropertyAccessFromIndexSignature,
            noUncheckedIndexedAccess: pkg.jsii?.tsc?.noUncheckedIndexedAccess,
            ..._sourceMapPreferences(pkg.jsii?.tsc),
            types: pkg.jsii?.tsc?.types,
        },
        bin: pkg.bin,
        exports: pkg.exports,
        diagnostics: _loadDiagnostics(pkg.jsii?.diagnostics),
        // user-provided tsconfig
        tsconfig: pkg.jsii?.tsconfig,
        validateTsconfig: _validateTsconfigRuleSet(pkg.jsii?.validateTsconfig ?? 'strict'),
    };
    return { projectInfo, diagnostics };
}
function _guessRepositoryType(url) {
    if (url.endsWith('.git')) {
        return 'git';
    }
    const parts = /^([^:]+):\/\//.exec(url);
    if (parts?.[1] !== 'http' && parts?.[1] !== 'https') {
        return parts[1];
    }
    throw new utils_1.JsiiError(`The "package.json" file must specify the "repository.type" attribute (could not guess from ${url})`);
}
function _sourceMapPreferences({ declarationMap, inlineSourceMap, inlineSources, sourceMap } = {}) {
    // If none of the options are specified, use the default configuration from jsii <= 1.58.0, which
    // means inline source maps with embedded source information.
    if (declarationMap == null && inlineSourceMap == null && inlineSources == null && sourceMap == null) {
        declarationMap = false;
        inlineSourceMap = true;
        inlineSources = true;
        sourceMap = undefined;
    }
    return {
        declarationMap,
        inlineSourceMap,
        inlineSources,
        sourceMap,
    };
}
class DependencyResolver {
    constructor() {
        this.cache = new Map();
    }
    /**
     * Discover the dependency tree starting at 'root', validating versions as we go along
     *
     * This primes the data structures in this class and should be called first.
     *
     * Return the resolved jsii dependency paths
     */
    discoverDependencyTree(root, dependencies) {
        const ret = {};
        for (const [name, declaration] of Object.entries(dependencies)) {
            // eslint-disable-next-line no-await-in-loop
            let resolved;
            try {
                resolved = this.resolveDependency(root, name, declaration);
            }
            catch (e) {
                LOG.error(`Unable to find a JSII dependency named "${name}" as "${declaration}". If you meant to include a non-JSII dependency, try adding it to bundledDependencies instead.`);
                throw e;
            }
            const actualVersion = resolved.dependencyInfo.assembly.version;
            if (!semver.satisfies(actualVersion, declaration)) {
                throw new utils_1.JsiiError(`Declared dependency on version ${declaration} of ${name}, but version ${actualVersion} was found`);
            }
            ret[name] = resolved.resolvedFile;
        }
        return ret;
    }
    /**
     * From a set of resolved paths, recursively return all assemblies
     */
    assemblyClosure(resolved) {
        const closure = new Map();
        const queue = Array.from(Object.values(resolved));
        while (queue.length > 0) {
            const next = queue.shift();
            const depInfo = this.cache.get(next);
            if (!depInfo) {
                throw new Error(`Path ${next} not seen before`);
            }
            if (closure.has(next)) {
                continue;
            }
            closure.set(next, depInfo.assembly);
            queue.push(...Object.values(depInfo.resolvedDependencies));
        }
        return Array.from(closure.values());
    }
    resolveDependency(root, name, declaration) {
        const { version: versionString, localPackage } = _resolveVersion(declaration, root);
        const version = new semver.Range(versionString);
        if (!version) {
            throw new utils_1.JsiiError(`Invalid semver expression for ${name}: ${versionString}`);
        }
        const jsiiFile = _tryResolveAssembly(name, localPackage, root);
        LOG.debug(`Resolved dependency ${name} to ${jsiiFile}`);
        return {
            resolvedVersion: versionString,
            resolvedFile: jsiiFile,
            dependencyInfo: this.loadAssemblyAndRecurse(jsiiFile),
        };
    }
    loadAssemblyAndRecurse(jsiiFile) {
        // Only recurse if we haven't seen this assembly yet
        if (this.cache.has(jsiiFile)) {
            return this.cache.get(jsiiFile);
        }
        const assembly = (0, spec_1.loadAssemblyFromFile)(jsiiFile);
        // Continue loading any dependencies declared in the asm
        const resolvedDependencies = assembly.dependencies
            ? this.discoverDependencyTree(path.dirname(jsiiFile), assembly.dependencies)
            : {};
        const depInfo = {
            assembly,
            resolvedDependencies,
        };
        this.cache.set(jsiiFile, depInfo);
        return depInfo;
    }
}
function _required(value, message) {
    if (value == null) {
        throw new utils_1.JsiiError(message);
    }
    return value;
}
function _toPerson(value, field, defaultRole = field) {
    if (typeof value === 'string') {
        value = (0, utils_1.parsePerson)(value);
    }
    return {
        name: _required(value.name, `The "package.json" file must specify the "${field}.name" attribute`),
        roles: value.roles ? [...new Set(value.roles)] : [defaultRole],
        email: value.email,
        url: value.url,
        organization: value.organization ? value.organization : undefined,
    };
}
function _toRepository(value) {
    if (typeof value === 'string') {
        value = (0, utils_1.parseRepository)(value);
    }
    return {
        url: _required(value.url, 'The "package.json" file must specify the "repository.url" attribute'),
        type: value.type || _guessRepositoryType(value.url),
        directory: value.directory,
    };
}
function _tryResolveAssembly(mod, localPackage, searchPath) {
    if (localPackage) {
        const result = (0, spec_1.findAssemblyFile)(localPackage);
        if (!fs.existsSync(result)) {
            throw new utils_1.JsiiError(`Assembly does not exist: ${result}`);
        }
        return result;
    }
    try {
        const dependencyDir = (0, find_utils_1.findDependencyDirectory)(mod, searchPath);
        return (0, spec_1.findAssemblyFile)(dependencyDir);
    }
    catch (e) {
        throw new utils_1.JsiiError(`Unable to locate jsii assembly for "${mod}". If this module is not jsii-enabled, it must also be declared under bundledDependencies: ${e}`);
    }
}
function _validateLicense(id) {
    if (id == null) {
        throw new utils_1.JsiiError('No "license" was specified in "package.json", see valid license identifiers at https://spdx.org/licenses/');
    }
    if (id === 'UNLICENSED') {
        return id;
    }
    if (!spdx.has(id)) {
        throw new utils_1.JsiiError(`Invalid license identifier "${id}", see valid license identifiers at https://spdx.org/licenses/`);
    }
    return id;
}
function _validateVersionFormat(format) {
    if (format !== 'short' && format !== 'full') {
        throw new utils_1.JsiiError(`Invalid jsii.versionFormat "${format}", it must be either "short" or "full" (the default)`);
    }
    return format;
}
function _validateStability(stability, deprecated) {
    if (!stability && deprecated) {
        stability = spec.Stability.Deprecated;
    }
    else if (deprecated && stability !== spec.Stability.Deprecated) {
        console.warn(`Package is deprecated (${deprecated}), but it's stability is ${stability} and not ${spec.Stability.Deprecated}`);
    }
    if (!stability) {
        return undefined;
    }
    if (!Object.values(spec.Stability).includes(stability)) {
        throw new utils_1.JsiiError(`Invalid stability "${stability}", it must be one of ${Object.values(spec.Stability).join(', ')}`);
    }
    return stability;
}
function _validateTsconfigRuleSet(ruleSet) {
    if (ruleSet == null) {
        return undefined;
    }
    if (!Object.values(tsconfig_1.TypeScriptConfigValidationRuleSet).includes(ruleSet)) {
        throw new utils_1.JsiiError(`Invalid validateTsconfig "${ruleSet}", it must be one of ${Object.values(tsconfig_1.TypeScriptConfigValidationRuleSet).join(', ')}`);
    }
    return ruleSet;
}
/**
 * Resolves an NPM package specifier to a version range
 *
 * If it was already a version range, return it. If it the
 * package references a local file, return the version that
 * package is at.
 */
function _resolveVersion(dep, searchPath) {
    const matches = /^file:(.+)$/.exec(dep);
    if (!matches) {
        return { version: dep };
    }
    const localPackage = path.resolve(searchPath, matches[1]);
    return {
        // Rendering as a caret version to maintain uniformity against the "standard".
        // eslint-disable-next-line @typescript-eslint/no-require-imports,@typescript-eslint/no-var-requires
        version: `^${JSON.parse(fs.readFileSync(path.join(localPackage, 'package.json'), 'utf-8')).version}`,
        localPackage,
    };
}
/**
 * Merges two metadata blocks together.
 *
 * @param base the base values
 * @param user the user-supplied values, which can override the `base` values
 *
 * @returns the merged metadata block
 */
function mergeMetadata(base, user) {
    if (user == null) {
        return base;
    }
    return mergeObjects(base, user);
    function mergeObjects(baseObj, override) {
        const result = {};
        const allKeys = Array.from(new Set([...Object.keys(baseObj), ...Object.keys(override)])).sort();
        for (const key of allKeys) {
            const baseValue = baseObj[key];
            const overrideValue = override[key];
            if (typeof baseValue === 'object' && typeof overrideValue === 'object') {
                if (overrideValue != null) {
                    result[key] = mergeObjects(baseValue, overrideValue);
                }
            }
            else {
                result[key] = overrideValue ?? baseValue;
            }
        }
        return result;
    }
}
function _loadDiagnostics(entries) {
    if (entries === undefined || Object.keys(entries).length === 0) {
        return undefined;
    }
    const result = {};
    for (const code of Object.keys(entries)) {
        let category;
        switch (entries[code].trim().toLowerCase()) {
            case 'error':
                category = ts.DiagnosticCategory.Error;
                break;
            case 'warning':
                category = ts.DiagnosticCategory.Warning;
                break;
            case 'suggestion':
                category = ts.DiagnosticCategory.Suggestion;
                break;
            case 'message':
                category = ts.DiagnosticCategory.Message;
                break;
            default:
                throw new utils_1.JsiiError(`Invalid category '${entries[code]}' for code '${code}'`);
        }
        result[code] = category;
    }
    return result;
}
function filterDictByKey(xs, predicate) {
    const ret = {};
    for (const [key, value] of Object.entries(xs)) {
        if (predicate(key)) {
            ret[key] = value;
        }
    }
    return ret;
}
//# sourceMappingURL=project-info.js.map