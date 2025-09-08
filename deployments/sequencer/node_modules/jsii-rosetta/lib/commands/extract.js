"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.extractAndInfuse = extractAndInfuse;
exports.extractSnippets = extractSnippets;
const path = require("node:path");
const infuse_1 = require("./infuse");
const assemblies_1 = require("../jsii/assemblies");
const logging = require("../logging");
const rosetta_translator_1 = require("../rosetta-translator");
const snippet_1 = require("../snippet");
const key_1 = require("../tablets/key");
const tablets_1 = require("../tablets/tablets");
const util_1 = require("../util");
async function extractAndInfuse(assemblyLocations, options) {
    const result = await extractSnippets(assemblyLocations, options);
    await (0, infuse_1.infuse)(assemblyLocations, {
        cacheFromFile: options.cacheFromFile,
        cacheToFile: options.cacheToFile,
        compressCacheToFile: options.compressCacheToFile,
    });
    return result;
}
/**
 * Extract all samples from the given assemblies into a tablet
 */
async function extractSnippets(assemblyLocations, options = {}) {
    const only = options.only ?? [];
    logging.info(`Loading ${assemblyLocations.length} assemblies`);
    const assemblies = (0, assemblies_1.loadAssemblies)(assemblyLocations, options.validateAssemblies ?? false);
    let snippets = Array.from(await (0, assemblies_1.allTypeScriptSnippets)(assemblies, options.loose));
    if (only.length > 0) {
        snippets = filterSnippets(snippets, only);
    }
    // Map every assembly to a list of snippets, so that we know what implicit
    // tablet to write the translations to later on.
    const snippetsPerAssembly = (0, util_1.groupBy)(snippets.map((s) => ({ key: (0, key_1.snippetKey)(s), location: projectDirectory(s) })), (x) => x.location);
    const translatorOptions = {
        includeCompilerDiagnostics: options.includeCompilerDiagnostics ?? false,
        assemblies: assemblies.map((a) => a.assembly),
        allowDirtyTranslations: options.allowDirtyTranslations,
    };
    const translator = options.translatorFactory
        ? options.translatorFactory(translatorOptions)
        : new rosetta_translator_1.RosettaTranslator(translatorOptions);
    // Prime the snippet cache with:
    // - Cache source file
    // - Default tablets found next to each assembly
    if (options.cacheFromFile) {
        await translator.addToCache(options.cacheFromFile);
    }
    translator.addTabletsToCache(...Object.values(await (0, assemblies_1.loadAllDefaultTablets)(assemblies)));
    if (translator.hasCache()) {
        const { translations, remaining } = translator.readFromCache(snippets, true, options.includeCompilerDiagnostics);
        logging.info(`Reused ${translations.length} translations from cache`);
        snippets = remaining;
    }
    const diagnostics = [];
    if (snippets.length > 0) {
        logging.info('Translating');
        const startTime = Date.now();
        const result = await translator.translateAll(snippets, {
            compilationDirectory: options.compilationDirectory,
            cleanup: options.cleanup,
        });
        const delta = (Date.now() - startTime) / 1000;
        logging.info(`Translated ${snippets.length} snippets in ${delta} seconds (${(delta / snippets.length).toPrecision(3)}s/snippet)`);
        diagnostics.push(...result.diagnostics);
    }
    else {
        logging.info('Nothing left to translate');
    }
    // Save to individual tablet files
    if (options.writeToImplicitTablets ?? true) {
        await Promise.all(Object.entries(snippetsPerAssembly).map(async ([location, snips]) => {
            // Compress the implicit tablet if explicitly asked to, otherwise compress only if the original tablet was compressed.
            const compressedTablet = options.compressTablet ?? (0, assemblies_1.compressedTabletExists)(location);
            const asmTabletFile = path.join(location, compressedTablet ? tablets_1.DEFAULT_TABLET_NAME_COMPRESSED : tablets_1.DEFAULT_TABLET_NAME);
            logging.debug(`Writing ${snips.length} translations to ${asmTabletFile}`);
            const translations = snips.map(({ key }) => translator.tablet.tryGetSnippet(key)).filter(util_1.isDefined);
            const asmTablet = new tablets_1.LanguageTablet();
            asmTablet.addSnippets(...translations);
            await asmTablet.save(asmTabletFile, compressedTablet);
        }));
    }
    // optionally append to the output file
    if (options.cacheToFile) {
        logging.info(`Adding translations to ${options.cacheToFile}`);
        const output = options.trimCache
            ? new tablets_1.LanguageTablet()
            : await tablets_1.LanguageTablet.fromOptionalFile(options.cacheToFile);
        output.addTablets(translator.tablet);
        await output.save(options.cacheToFile, options.compressCacheToFile);
    }
    return { diagnostics, tablet: translator.tablet };
}
/**
 * Only yield the snippets whose id exists in a whitelist
 */
function filterSnippets(ts, includeIds) {
    return ts.filter((t) => includeIds.includes((0, key_1.snippetKey)(t)));
}
function projectDirectory(ts) {
    const dir = ts.parameters?.[snippet_1.SnippetParameters.$PROJECT_DIRECTORY];
    if (!dir) {
        throw new Error(`Snippet does not have associated project directory: ${JSON.stringify(ts.location)}`);
    }
    return dir;
}
//# sourceMappingURL=extract.js.map