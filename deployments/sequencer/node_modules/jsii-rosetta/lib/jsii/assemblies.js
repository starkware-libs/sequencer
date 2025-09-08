"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.EXAMPLE_METADATA_JSDOCTAG = void 0;
exports.loadAssemblies = loadAssemblies;
exports.loadAllDefaultTablets = loadAllDefaultTablets;
exports.guessTabletLocation = guessTabletLocation;
exports.compressedTabletExists = compressedTabletExists;
exports.allSnippetSources = allSnippetSources;
exports.allTypeScriptSnippets = allTypeScriptSnippets;
exports.findTypeLookupAssembly = findTypeLookupAssembly;
exports.findContainingSubmodule = findContainingSubmodule;
const node_fs_1 = require("node:fs");
const fs = require("node:fs");
const path = require("node:path");
const spec_1 = require("@jsii/spec");
const spec = require("@jsii/spec");
const fixtures_1 = require("../fixtures");
const extract_snippets_1 = require("../markdown/extract-snippets");
const snippet_1 = require("../snippet");
const snippet_dependencies_1 = require("../snippet-dependencies");
const strict_1 = require("../strict");
const tablets_1 = require("../tablets/tablets");
const util_1 = require("../util");
/**
 * The JSDoc tag users can use to associate non-visible metadata with an example
 *
 * In a Markdown section, metadata goes after the code block fence, where it will
 * be attached to the example but invisible.
 *
 *    ```ts metadata=goes here
 *
 * But in doc comments, '@example' already delineates the example, and any metadata
 * in there added by the '///' tags becomes part of the visible code (there is no
 * place to put hidden information).
 *
 * We introduce the '@exampleMetadata' tag to put that additional information.
 */
exports.EXAMPLE_METADATA_JSDOCTAG = 'exampleMetadata';
/**
 * Load assemblies by filename or directory
 */
function loadAssemblies(assemblyLocations, validateAssemblies) {
    return assemblyLocations.map(loadAssembly);
    function loadAssembly(location) {
        const stat = fs.statSync(location);
        if (stat.isDirectory()) {
            return loadAssembly((0, spec_1.findAssemblyFile)(location));
        }
        const directory = path.dirname(location);
        const pjLocation = path.join(directory, 'package.json');
        const assembly = (0, spec_1.loadAssemblyFromFile)(location, validateAssemblies);
        const packageJson = fs.existsSync(pjLocation) ? JSON.parse(fs.readFileSync(pjLocation, 'utf-8')) : undefined;
        return { assembly, directory, packageJson };
    }
}
/**
 * Load the default tablets for every assembly, if available
 *
 * Returns a map of { directory -> tablet }.
 */
async function loadAllDefaultTablets(asms) {
    return (0, util_1.mkDict)(await Promise.all(asms.map(async (a) => [a.directory, await tablets_1.LanguageTablet.fromOptionalFile(guessTabletLocation(a.directory))])));
}
/**
 * Returns the location of the tablet file, either .jsii.tabl.json or .jsii.tabl.json.gz.
 * Assumes that a tablet exists in the directory and if not, the ensuing behavior is
 * handled by the caller of this function.
 */
function guessTabletLocation(directory) {
    return compressedTabletExists(directory)
        ? path.join(directory, tablets_1.DEFAULT_TABLET_NAME_COMPRESSED)
        : path.join(directory, tablets_1.DEFAULT_TABLET_NAME);
}
function compressedTabletExists(directory) {
    return fs.existsSync(path.join(directory, tablets_1.DEFAULT_TABLET_NAME_COMPRESSED));
}
/**
 * Return all markdown and example snippets from the given assembly
 */
function allSnippetSources(assembly) {
    const ret = [];
    if (assembly.readme) {
        ret.push({
            type: 'markdown',
            markdown: assembly.readme.markdown,
            location: { api: 'moduleReadme', moduleFqn: assembly.name },
        });
    }
    for (const [submoduleFqn, submodule] of Object.entries(assembly.submodules ?? {})) {
        if (submodule.readme) {
            ret.push({
                type: 'markdown',
                markdown: submodule.readme.markdown,
                location: { api: 'moduleReadme', moduleFqn: submoduleFqn },
            });
        }
    }
    if (assembly.types) {
        for (const type of Object.values(assembly.types)) {
            emitDocs(type.docs, { api: 'type', fqn: type.fqn });
            if (spec.isEnumType(type)) {
                for (const m of type.members)
                    emitDocs(m.docs, { api: 'member', fqn: type.fqn, memberName: m.name });
            }
            if (spec.isClassType(type)) {
                emitDocsForCallable(type.initializer, type.fqn);
            }
            if (spec.isClassOrInterfaceType(type)) {
                for (const m of type.methods ?? [])
                    emitDocsForCallable(m, type.fqn, m.name);
                for (const m of type.properties ?? [])
                    emitDocs(m.docs, { api: 'member', fqn: type.fqn, memberName: m.name });
            }
        }
    }
    return ret;
    function emitDocsForCallable(callable, fqn, memberName) {
        if (!callable) {
            return;
        }
        emitDocs(callable.docs, memberName ? { api: 'member', fqn, memberName } : { api: 'initializer', fqn });
        for (const parameter of callable.parameters ?? []) {
            emitDocs(parameter.docs, {
                api: 'parameter',
                fqn: fqn,
                methodName: memberName ?? snippet_1.INITIALIZER_METHOD_NAME,
                parameterName: parameter.name,
            });
        }
    }
    function emitDocs(docs, location) {
        if (!docs) {
            return;
        }
        if (docs.remarks) {
            ret.push({
                type: 'markdown',
                markdown: docs.remarks,
                location,
            });
        }
        if (docs.example) {
            ret.push({
                type: 'example',
                source: docs.example,
                metadata: (0, util_1.fmap)(docs.custom?.[exports.EXAMPLE_METADATA_JSDOCTAG], snippet_1.parseMetadataLine),
                location,
            });
        }
    }
}
async function allTypeScriptSnippets(assemblies, loose = false) {
    const sources = assemblies
        .flatMap((loaded) => allSnippetSources(loaded.assembly).map((source) => ({ source, loaded })))
        .flatMap(({ source, loaded }) => {
        switch (source.type) {
            case 'example':
                return [
                    {
                        snippet: (0, snippet_1.updateParameters)((0, snippet_1.typeScriptSnippetFromVisibleSource)(source.source, { api: source.location, field: { field: 'example' } }, isStrict(loaded)), source.metadata ?? {}),
                        loaded,
                    },
                ];
            case 'markdown':
                return (0, extract_snippets_1.extractTypescriptSnippetsFromMarkdown)(source.markdown, source.location, isStrict(loaded)).map((snippet) => ({ snippet, loaded }));
        }
    });
    const fixtures = [];
    for (let { snippet, loaded } of sources) {
        const isInfused = snippet.parameters?.infused != null;
        // Ignore fixturization errors if requested on this command, or if the snippet was infused
        const ignoreFixtureErrors = loose || isInfused;
        // Also if the snippet was infused: switch off 'strict' mode if it was set
        if (isInfused) {
            snippet = { ...snippet, strict: false };
        }
        snippet = await withDependencies(loaded, withProjectDirectory(loaded.directory, snippet));
        fixtures.push((0, fixtures_1.fixturize)(snippet, ignoreFixtureErrors));
    }
    return fixtures;
}
const MAX_ASM_CACHE = 3;
const ASM_CACHE = [];
/**
 * Recursively searches for a .jsii file in the directory.
 * When file is found, checks cache to see if we already
 * stored the assembly in memory. If not, we synchronously
 * load the assembly into memory.
 */
function findTypeLookupAssembly(startingDirectory) {
    const pjLocation = findPackageJsonLocation(path.resolve(startingDirectory));
    if (!pjLocation) {
        return undefined;
    }
    const directory = path.dirname(pjLocation);
    const fromCache = ASM_CACHE.find((c) => c.directory === directory);
    if (fromCache) {
        return fromCache;
    }
    const loaded = loadLookupAssembly(directory);
    if (!loaded) {
        return undefined;
    }
    while (ASM_CACHE.length >= MAX_ASM_CACHE) {
        ASM_CACHE.pop();
    }
    ASM_CACHE.unshift(loaded);
    return loaded;
}
function loadLookupAssembly(directory) {
    try {
        const packageJson = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf-8'));
        const assembly = (0, spec_1.loadAssemblyFromPath)(directory);
        const symbolIdMap = (0, util_1.mkDict)([
            ...Object.values(assembly.types ?? {}).map((type) => [type.symbolId ?? '', type.fqn]),
            ...Object.entries(assembly.submodules ?? {}).map(([fqn, mod]) => [mod.symbolId ?? '', fqn]),
        ]);
        return {
            packageJson,
            assembly,
            directory,
            symbolIdMap,
        };
    }
    catch {
        return undefined;
    }
}
function findPackageJsonLocation(currentPath) {
    // eslint-disable-next-line no-constant-condition
    while (true) {
        const candidate = path.join(currentPath, 'package.json');
        if (fs.existsSync(candidate)) {
            return candidate;
        }
        const parentPath = path.resolve(currentPath, '..');
        if (parentPath === currentPath) {
            return undefined;
        }
        currentPath = parentPath;
    }
}
/**
 * Find the jsii [sub]module that contains the given FQN
 *
 * @returns `undefined` if the type is a member of the assembly root.
 */
function findContainingSubmodule(assembly, fqn) {
    const submoduleNames = Object.keys(assembly.submodules ?? {});
    (0, util_1.sortBy)(submoduleNames, (s) => [-s.length]); // Longest first
    for (const s of submoduleNames) {
        if (fqn.startsWith(`${s}.`)) {
            return s;
        }
    }
    return undefined;
}
function withProjectDirectory(dir, snippet) {
    return (0, snippet_1.updateParameters)(snippet, {
        [snippet_1.SnippetParameters.$PROJECT_DIRECTORY]: dir,
    });
}
/**
 * Return a TypeScript snippet with dependencies added
 *
 * The dependencies will be taken from the package.json, and will consist of:
 *
 * - The package itself
 * - The package's dependencies and peerDependencies (but NOT devDependencies). Will
 *   symlink to the files on disk.
 * - Any additional dependencies declared in `jsiiRosetta.exampleDependencies`.
 */
async function withDependencies(asm, snippet) {
    const compilationDependencies = {};
    if (await (0, util_1.pathExists)(path.join(asm.directory, 'package.json'))) {
        compilationDependencies[asm.assembly.name] = {
            type: 'concrete',
            resolvedDirectory: await node_fs_1.promises.realpath(asm.directory),
        };
    }
    Object.assign(compilationDependencies, await (0, snippet_dependencies_1.resolveDependenciesFromPackageJson)(asm.packageJson, asm.directory));
    Object.assign(compilationDependencies, (0, util_1.mkDict)(Object.entries(asm.packageJson?.jsiiRosetta?.exampleDependencies ?? {}).map(([name, versionRange]) => [name, { type: 'symbolic', versionRange }])));
    return {
        ...snippet,
        compilationDependencies,
    };
}
/**
 * Whether samples in the assembly should be treated as strict
 *
 * True if the strict flag is found in the package.json (modern) or the assembly itself (legacy).
 */
function isStrict(loaded) {
    return loaded.packageJson?.jsiiRosetta?.strict ?? (0, strict_1.enforcesStrictMode)(loaded.assembly);
}
//# sourceMappingURL=assemblies.js.map