"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.transliterateAssembly = transliterateAssembly;
const node_fs_1 = require("node:fs");
const node_path_1 = require("node:path");
const spec_1 = require("@jsii/spec");
const extract_1 = require("./extract");
const target_language_1 = require("../languages/target-language");
const logging_1 = require("../logging");
const rosetta_reader_1 = require("../rosetta-reader");
const snippet_1 = require("../snippet");
/**
 * Prepares transliterated versions of the designated assemblies into the
 * selected target languages.
 *
 * @param assemblyLocations the directories which contain assemblies to
 *                          transliterate.
 * @param targetLanguages   the languages into which to transliterate.
 * @param tabletLocation    an optional Rosetta tablet file to source
 *                          pre-transliterated snippets from.
 *
 * @experimental
 */
async function transliterateAssembly(assemblyLocations, targetLanguages, options = {}) {
    // Start by doing an 'extract' for all these assemblies
    //
    // This will locate all examples that haven't been translated yet and translate
    // them. Importantly: it will translate them in parallel, which is going to improve
    // performance a lot. We ignore diagnostics.
    const { tablet } = await (0, extract_1.extractSnippets)(assemblyLocations, {
        includeCompilerDiagnostics: true,
        loose: options.loose,
        cacheFromFile: options.tablet,
        writeToImplicitTablets: false,
        allowDirtyTranslations: true,
    });
    // Now do a regular "tablet reader" cycle, expecting everything to be translated already,
    // and therefore it doesn't matter that we do this all in a single-threaded loop.
    const rosetta = new rosetta_reader_1.RosettaTabletReader({
        unknownSnippets: options?.unknownSnippets ?? rosetta_reader_1.UnknownSnippetMode.FAIL,
        targetLanguages,
        prefixDisclaimer: true,
    });
    // Put in the same caching tablet here
    if (options.tablet) {
        await rosetta.loadTabletFromFile(options.tablet);
    }
    // Any fresh translations we just came up with
    rosetta.addTablet(tablet);
    const assemblies = await loadAssemblies(assemblyLocations, rosetta);
    for (const [location, loadAssembly] of assemblies.entries()) {
        for (const language of targetLanguages) {
            const now = new Date().getTime();
            const result = loadAssembly();
            if (result.targets?.[(0, target_language_1.targetName)(language)] == null) {
                // This language is not supported by the assembly, so we skip it...
                continue;
            }
            if (result.readme?.markdown) {
                result.readme.markdown = rosetta.translateSnippetsInMarkdown({ api: 'moduleReadme', moduleFqn: result.name }, result.readme.markdown, language, true /* strict */);
            }
            for (const type of Object.values(result.types ?? {})) {
                transliterateType(type, rosetta, language);
            }
            // eslint-disable-next-line no-await-in-loop
            await node_fs_1.promises.writeFile((0, node_path_1.resolve)(options?.outdir ?? location, `${spec_1.SPEC_FILE_NAME}.${language}`), JSON.stringify(result, null, 2));
            const then = new Date().getTime();
            (0, logging_1.debug)(`Done transliterating ${result.name}@${result.version} to ${language} after ${then - now} milliseconds`);
        }
    }
    rosetta.printDiagnostics(process.stderr, process.stderr.isTTY);
    if (rosetta.hasErrors && options.strict) {
        throw new Error('Strict mode is enabled and some examples failed compilation!');
    }
}
/**
 * Given a set of directories containing `.jsii` assemblies, load all the
 * assemblies into the provided `Rosetta` instance and return a map of
 * directories to assembly-loading functions (the function re-loads the original
 * assembly from disk on each invocation).
 *
 * @param directories the assembly-containing directories to traverse.
 * @param rosetta     the `Rosetta` instance in which to load assemblies.
 *
 * @returns a map of directories to a function that loads the `.jsii` assembly
 *          contained therein from disk.
 */
async function loadAssemblies(directories, rosetta) {
    const result = new Map();
    for (const directory of directories) {
        const loader = () => (0, spec_1.loadAssemblyFromPath)(directory);
        // eslint-disable-next-line no-await-in-loop
        await rosetta.addAssembly(loader(), directory);
        result.set(directory, loader);
    }
    return result;
}
function transliterateType(type, rosetta, language) {
    transliterateDocs({ api: 'type', fqn: type.fqn }, type.docs);
    switch (type.kind) {
        // eslint-disable-next-line @typescript-eslint/ban-ts-comment
        // @ts-ignore 7029
        case spec_1.TypeKind.Class:
            if (type.initializer) {
                transliterateDocs({ api: 'initializer', fqn: type.fqn }, type.initializer.docs);
            }
        // fallthrough
        case spec_1.TypeKind.Interface:
            for (const method of type.methods ?? []) {
                transliterateDocs({ api: 'member', fqn: type.fqn, memberName: method.name }, method.docs);
                for (const parameter of method.parameters ?? []) {
                    transliterateDocs({ api: 'parameter', fqn: type.fqn, methodName: method.name, parameterName: parameter.name }, parameter.docs);
                }
            }
            for (const property of type.properties ?? []) {
                transliterateDocs({ api: 'member', fqn: type.fqn, memberName: property.name }, property.docs);
            }
            break;
        case spec_1.TypeKind.Enum:
            for (const member of type.members) {
                transliterateDocs({ api: 'member', fqn: type.fqn, memberName: member.name }, member.docs);
            }
            break;
        default:
            throw new Error(`Unsupported type kind: ${type.kind}`);
    }
    function transliterateDocs(api, docs) {
        if (docs?.remarks) {
            docs.remarks = rosetta.translateSnippetsInMarkdown(api, docs.remarks, language, true /* strict */);
        }
        if (docs?.example) {
            const location = { api, field: { field: 'example' } };
            const snippet = (0, snippet_1.typeScriptSnippetFromVisibleSource)(docs.example, location, true /* strict */);
            const translation = rosetta.translateSnippet(snippet, language);
            if (translation != null) {
                docs.example = translation.source;
            }
        }
    }
}
//# sourceMappingURL=transliterate.js.map