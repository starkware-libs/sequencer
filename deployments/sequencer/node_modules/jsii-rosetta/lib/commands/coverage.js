"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.checkCoverage = checkCoverage;
const assemblies_1 = require("../jsii/assemblies");
const logging = require("../logging");
const rosetta_translator_1 = require("../rosetta-translator");
const snippet_1 = require("../snippet");
async function checkCoverage(assemblyLocations) {
    logging.info(`Loading ${assemblyLocations.length} assemblies`);
    const assemblies = (0, assemblies_1.loadAssemblies)(assemblyLocations, false);
    const snippets = Array.from(await (0, assemblies_1.allTypeScriptSnippets)(assemblies, true));
    const translator = new rosetta_translator_1.RosettaTranslator({
        assemblies: assemblies.map((a) => a.assembly),
        allowDirtyTranslations: true,
    });
    translator.addTabletsToCache(...Object.values(await (0, assemblies_1.loadAllDefaultTablets)(assemblies)));
    process.stdout.write(`- ${snippets.length} total snippets.\n`);
    process.stdout.write(`- ${translator.cache.count} translations in cache.\n`);
    process.stdout.write('\n');
    const results = translator.readFromCache(snippets, true, true);
    process.stdout.write(`- ${results.translations.length - results.dirtyCount} successful cache hits.\n`);
    process.stdout.write(`     ${results.infusedCount} infused.\n`);
    process.stdout.write(`- ${results.dirtyCount} translations in cache but dirty (ok for pacmak, transliterate)\n`);
    process.stdout.write(`     ${results.dirtySourceCount} sources have changed.\n`);
    process.stdout.write(`     ${results.dirtyTranslatorCount} translator has changed.\n`);
    process.stdout.write(`     ${results.dirtyTypesCount} types have changed.\n`);
    process.stdout.write(`     ${results.dirtyDidntCompile} did not successfully compile.\n`);
    process.stdout.write(`- ${results.remaining.length} snippets untranslated.\n`);
    for (const remaining of results.remaining) {
        process.stdout.write(`     ${(0, snippet_1.formatLocation)(remaining.location)}\n`);
    }
}
//# sourceMappingURL=coverage.js.map