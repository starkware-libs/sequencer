"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.RosettaTranslator = void 0;
const node_fs_1 = require("node:fs");
const fingerprinting_1 = require("./jsii/fingerprinting");
const languages_1 = require("./languages");
const logging = require("./logging");
const snippet_1 = require("./snippet");
const snippet_dependencies_1 = require("./snippet-dependencies");
const key_1 = require("./tablets/key");
const tablets_1 = require("./tablets/tablets");
const translate_all_1 = require("./translate_all");
/**
 * Entry point for consumers that want to translate code on-the-fly
 *
 * If you want to generate and translate code on-the-fly, in ways that cannot
 * be achieved by the rosetta CLI, use this class.
 */
class RosettaTranslator {
    constructor(options = {}) {
        /**
         * Tablet with fresh translations
         *
         * All new translations (not read from cache) are added to this tablet.
         */
        this.tablet = new tablets_1.LanguageTablet();
        this.cache = new tablets_1.LanguageTablet();
        this.fingerprinter = new fingerprinting_1.TypeFingerprinter(options?.assemblies ?? []);
        this.includeCompilerDiagnostics = options.includeCompilerDiagnostics ?? false;
        this.allowDirtyTranslations = options.allowDirtyTranslations ?? false;
    }
    /**
     * @deprecated use `addToCache` instead
     */
    async loadCache(fileName) {
        try {
            await this.cache.load(fileName);
        }
        catch (e) {
            logging.warn(`Error reading cache ${fileName}: ${e.message}`);
        }
    }
    async addToCache(filename) {
        const tab = await tablets_1.LanguageTablet.fromOptionalFile(filename);
        this.cache.addTablet(tab);
    }
    addTabletsToCache(...tablets) {
        for (const tab of tablets) {
            this.cache.addTablet(tab);
        }
    }
    hasCache() {
        return this.cache.count > 0;
    }
    /**
     * For all the given snippets, try to read translations from the cache
     *
     * Will remove the cached snippets from the input array.
     */
    readFromCache(snippets, addToTablet = true, compiledOnly = false) {
        const translations = new Array();
        const remaining = new Array();
        let infusedCount = 0;
        let dirtyCount = 0;
        let dirtySourceCount = 0;
        let dirtyTypesCount = 0;
        let dirtyTranslatorCount = 0;
        let dirtyDidntCompile = 0;
        for (const snippet of snippets) {
            const fromCache = tryReadFromCache(snippet, this.cache, this.fingerprinter, compiledOnly);
            switch (fromCache.type) {
                case 'hit':
                    if (addToTablet) {
                        this.tablet.addSnippet(fromCache.snippet);
                    }
                    translations.push(fromCache.snippet);
                    infusedCount += fromCache.infused ? 1 : 0;
                    break;
                case 'dirty':
                    dirtyCount += 1;
                    dirtySourceCount += fromCache.dirtySource ? 1 : 0;
                    dirtyTranslatorCount += fromCache.dirtyTranslator ? 1 : 0;
                    dirtyTypesCount += fromCache.dirtyTypes ? 1 : 0;
                    dirtyDidntCompile += fromCache.dirtyDidntCompile ? 1 : 0;
                    if (this.allowDirtyTranslations) {
                        translations.push(fromCache.translation);
                    }
                    else {
                        remaining.push(snippet);
                    }
                    break;
                case 'miss':
                    remaining.push(snippet);
                    break;
            }
        }
        return {
            translations,
            remaining,
            infusedCount,
            dirtyCount,
            dirtySourceCount,
            dirtyTranslatorCount,
            dirtyTypesCount,
            dirtyDidntCompile,
        };
    }
    async translateAll(snippets, optionsOrAddToTablet) {
        const options = optionsOrAddToTablet && typeof optionsOrAddToTablet === 'object'
            ? optionsOrAddToTablet
            : { addToTablet: optionsOrAddToTablet };
        const exampleDependencies = (0, snippet_dependencies_1.collectDependencies)(snippets);
        await (0, snippet_dependencies_1.expandWithTransitiveDependencies)(exampleDependencies);
        let compilationDirectory;
        let cleanCompilationDir = false;
        if (options?.compilationDirectory) {
            // If the user provided a directory, we're going to trust-but-confirm.
            await (0, snippet_dependencies_1.validateAvailableDependencies)(options.compilationDirectory, exampleDependencies);
            compilationDirectory = options.compilationDirectory;
        }
        else {
            compilationDirectory = await (0, snippet_dependencies_1.prepareDependencyDirectory)(exampleDependencies);
            cleanCompilationDir = true;
        }
        const origDir = process.cwd();
        // Easiest way to get a fixed working directory (for sources) in is to chdir
        process.chdir(compilationDirectory);
        let result;
        try {
            result = await (0, translate_all_1.translateAll)(snippets, this.includeCompilerDiagnostics);
        }
        finally {
            process.chdir(origDir);
            if (cleanCompilationDir) {
                if (options.cleanup ?? true) {
                    await node_fs_1.promises.rm(compilationDirectory, { force: true, recursive: true });
                }
                else {
                    logging.info(`Leaving directory uncleaned: ${compilationDirectory}`);
                }
            }
        }
        const fingerprinted = result.translatedSnippets.map((snippet) => snippet.withFingerprint(this.fingerprinter.fingerprintAll(snippet.fqnsReferenced())));
        if (options?.addToTablet ?? true) {
            for (const translation of fingerprinted) {
                this.tablet.addSnippet(translation);
            }
        }
        return {
            translatedSnippets: fingerprinted,
            diagnostics: result.diagnostics,
        };
    }
}
exports.RosettaTranslator = RosettaTranslator;
/**
 * Try to find the translation for the given snippet in the given cache
 *
 * Rules for cacheability are:
 * - id is the same (== visible source didn't change)
 * - complete source is the same (== fixture didn't change)
 * - all types involved have the same fingerprint (== API surface didn't change)
 * - the versions of all translations match the versions on the available translators (== translator itself didn't change)
 *
 * For the versions check: we could have selectively picked some translations
 * from the cache while performing others. However, since the big work is in
 * parsing the TypeScript, and the rendering itself is peanutes (assumption), it
 * doesn't really make a lot of difference.  So, for simplification's sake,
 * we'll regen all translations if there's at least one that's outdated.
 */
function tryReadFromCache(sourceSnippet, cache, fingerprinter, compiledOnly) {
    const fromCache = cache.tryGetSnippet((0, key_1.snippetKey)(sourceSnippet));
    if (!fromCache) {
        return { type: 'miss' };
    }
    // infused snippets won't pass the full source check or the fingerprinter
    // but there is no reason to try to recompile it, so return cached snippet
    // if there exists one.
    if (isInfused(sourceSnippet)) {
        return { type: 'hit', snippet: fromCache, infused: true };
    }
    const dirtySource = (0, snippet_1.completeSource)(sourceSnippet) !== fromCache.snippet.fullSource;
    const dirtyTranslator = !Object.entries(languages_1.TARGET_LANGUAGES).every(([lang, translator]) => fromCache.snippet.translations?.[lang]?.version === translator.version);
    const dirtyTypes = fingerprinter.fingerprintAll(fromCache.fqnsReferenced()) !== fromCache.snippet.fqnsFingerprint;
    const dirtyDidntCompile = compiledOnly && !fromCache.snippet.didCompile;
    if (dirtySource || dirtyTranslator || dirtyTypes || dirtyDidntCompile) {
        return { type: 'dirty', translation: fromCache, dirtySource, dirtyTranslator, dirtyTypes, dirtyDidntCompile };
    }
    return { type: 'hit', snippet: fromCache, infused: false };
}
function isInfused(snippet) {
    return snippet.parameters?.infused !== undefined;
}
//# sourceMappingURL=rosetta-translator.js.map