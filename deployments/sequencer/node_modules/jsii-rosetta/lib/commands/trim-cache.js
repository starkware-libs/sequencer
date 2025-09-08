"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.trimCache = trimCache;
const assemblies_1 = require("../jsii/assemblies");
const logging = require("../logging");
const key_1 = require("../tablets/key");
const tablets_1 = require("../tablets/tablets");
const util_1 = require("../util");
async function trimCache(options) {
    logging.info(`Loading ${options.assemblyLocations.length} assemblies`);
    const assemblies = (0, assemblies_1.loadAssemblies)(options.assemblyLocations, false);
    const snippets = Array.from(await (0, assemblies_1.allTypeScriptSnippets)(assemblies));
    const original = await tablets_1.LanguageTablet.fromFile(options.cacheFile);
    const updated = new tablets_1.LanguageTablet();
    updated.addSnippets(...snippets.map((snip) => original.tryGetSnippet((0, key_1.snippetKey)(snip))).filter(util_1.isDefined));
    // if the original file was compressed, then compress the updated file too
    await updated.save(options.cacheFile, original.compressedSource);
    // eslint-disable-next-line prettier/prettier
    logging.info(`${options.cacheFile}: ${updated.count} snippets remaining (${original.count} - ${updated.count} trimmed)`);
}
//# sourceMappingURL=trim-cache.js.map