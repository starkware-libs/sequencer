"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DEFAULT_INFUSION_RESULTS_NAME = void 0;
exports.infuse = infuse;
const fs = require("node:fs");
const path = require("node:path");
const spec = require("@jsii/spec");
const spec_1 = require("@jsii/spec");
const assemblies_1 = require("../jsii/assemblies");
const snippet_1 = require("../snippet");
const snippet_selectors_1 = require("../snippet-selectors");
const key_1 = require("../tablets/key");
const tablets_1 = require("../tablets/tablets");
const util_1 = require("../util");
exports.DEFAULT_INFUSION_RESULTS_NAME = 'infusion-results.html';
const ADDITIONAL_SELECTORS = { meanLength: snippet_selectors_1.meanLength, shortest: snippet_selectors_1.shortest, longest: snippet_selectors_1.longest };
class DefaultRecord {
    constructor() {
        this.index = {};
    }
    add(key, value) {
        if (!this.index[key]) {
            this.index[key] = [];
        }
        this.index[key].push(value);
    }
}
/**
 * Infuse will analyze the snippets in a set of tablets, and update the assembly to add
 * examples to types that don't have any yet, based on snippets that use the given type.
 */
async function infuse(assemblyLocations, options) {
    let stream = undefined;
    if (options?.logFile) {
        // Create stream for html file and insert some styling
        stream = fs.createWriteStream(options.logFile, { encoding: 'utf-8' });
        startFile(stream);
    }
    // Load tablet file and assemblies
    const assemblies = (0, assemblies_1.loadAssemblies)(assemblyLocations, false);
    const defaultTablets = await (0, assemblies_1.loadAllDefaultTablets)(assemblies);
    const availableTranslations = new tablets_1.LanguageTablet();
    if (options?.cacheFromFile) {
        availableTranslations.addTablet(await tablets_1.LanguageTablet.fromOptionalFile(options.cacheFromFile));
    }
    availableTranslations.addTablets(...Object.values(defaultTablets));
    const { translationsByFqn, originalsByKey } = await availableSnippetsPerFqn(assemblies, availableTranslations);
    const additionalOutputTablet = options?.cacheToFile
        ? await tablets_1.LanguageTablet.fromOptionalFile(options?.cacheToFile)
        : new tablets_1.LanguageTablet();
    const coverageResults = (0, util_1.mkDict)(await Promise.all(assemblies.map(async ({ assembly, directory }) => {
        stream?.write(`<h1>${assembly.name}</h1>\n`);
        const implicitTablet = defaultTablets[directory];
        const implicitTabletFile = path.join(directory, implicitTablet.compressedSource ? tablets_1.DEFAULT_TABLET_NAME_COMPRESSED : tablets_1.DEFAULT_TABLET_NAME);
        if (!implicitTablet) {
            throw new Error(`No tablet found for ${directory}`);
        }
        let insertedExamples = 0;
        const filteredTypes = filterForTypesWithoutExamples(assembly.types ?? {});
        for (const [typeFqn, type] of Object.entries(filteredTypes)) {
            const available = translationsByFqn[typeFqn];
            if (!available) {
                continue;
            }
            const example = pickBestExample(typeFqn, available, stream);
            const original = originalsByKey[example.key];
            insertExample(example, original, type, [implicitTablet, additionalOutputTablet]);
            insertedExamples++;
        }
        if (insertedExamples > 0) {
            // Save the updated assembly and implicit tablets
            // eslint-disable-next-line no-await-in-loop
            await Promise.all([
                (0, spec_1.replaceAssembly)(assembly, directory),
                implicitTablet.save(implicitTabletFile, implicitTablet.compressedSource),
            ]);
        }
        return [
            directory,
            {
                types: Object.keys(filteredTypes).length,
                typesWithInsertedExamples: insertedExamples,
            },
        ];
    })));
    stream?.close();
    // If we copied examples onto different types, we'll also have inserted new snippets
    // with different keys into the tablet. We must now write the updated tablet somewhere.
    if (options?.cacheToFile) {
        await additionalOutputTablet.save(options.cacheToFile, options.compressCacheToFile);
    }
    return {
        coverageResults: coverageResults,
    };
}
function pickBestExample(typeFqn, choices, logStream) {
    const meanResult = (0, snippet_selectors_1.mean)(choices);
    if (logStream) {
        const selected = Object.entries(ADDITIONAL_SELECTORS).map(([name, fn]) => [name, fn(choices)]);
        const selectedFromSelector = {
            ...makeDict(selected),
            mean: meanResult,
        };
        logOutput(logStream, typeFqn, createHtmlEntry(selectedFromSelector));
    }
    return meanResult;
}
function startFile(stream) {
    stream.write('<style>\n');
    stream.write('h2 { color: blue; clear: both; }\n');
    stream.write('h1 { color: red; clear: both; }\n');
    stream.write('div { float: left; height: 31em; width: 22em; overflow: auto; margin: 1em; background-color: #ddd; }\n');
    stream.write('pre { float: left; height: 30em; width: 25em; overflow: auto; padding: 0.5em; background-color: #ddd; }\n');
    stream.write('</style>\n');
}
function createHtmlEntry(results) {
    const entry = new DefaultRecord();
    for (const [key, value] of Object.entries(results)) {
        entry.add(value.originalSource.source, key);
    }
    return entry.index;
}
function logOutput(stream, typeFqn, algorithmMap) {
    stream?.write(`<h2>${typeFqn}</h2>\n`);
    for (const [key, value] of Object.entries(algorithmMap)) {
        stream?.write(`<div class="snippet"><h3>${value.toString()}</h3>\n<pre>${key}</pre>\n</div>\n`);
    }
    for (let i = 0; i < 4 - Object.keys(algorithmMap).length; i++) {
        stream?.write('<div class="padding"></div>\n');
    }
}
function filterForTypesWithoutExamples(types) {
    const filteredTypes = {};
    for (const [typeFqn, type] of Object.entries(types)) {
        // Ignore Interfaces that contain only properties
        if (type.kind === spec.TypeKind.Interface && !type.datatype) {
            continue;
        }
        // Already has example
        if (type.docs?.example !== undefined) {
            continue;
        }
        filteredTypes[typeFqn] = type;
    }
    return filteredTypes;
}
/**
 * Insert an example into the docs of a type, and insert it back into the tablet under a new key
 */
function insertExample(example, original, type, tablets) {
    const parameters = {
        ...original?.parameters,
        infused: '',
    };
    // exampleMetadata should always be nonempty since we always have a parameter.
    const exampleMetadata = (0, snippet_1.renderMetadataline)(parameters) ?? '';
    if (type.docs) {
        type.docs.example = example.originalSource.source;
        type.docs.custom = { ...type.docs.custom, exampleMetadata };
    }
    else {
        type.docs = {
            example: example.originalSource.source,
            custom: { exampleMetadata },
        };
    }
    for (const tablet of tablets) {
        tablet.addSnippet(example.withLocation({
            api: { api: 'type', fqn: type.fqn },
            field: { field: 'example' },
        }));
    }
}
/**
 * Return a map of FQN -> snippet keys that exercise that FQN.
 *
 * For a snippet to qualify, it must both:
 *
 * a) be current (i.e.: exist in the input assemblies)
 * b) have been analyzed (i.e.: exist in one of the translated tablets)
 *
 * Returns a map of fqns to a list of keys that represent snippets that include the fqn.
 */
async function availableSnippetsPerFqn(asms, translationsTablet) {
    const ret = new DefaultRecord();
    const originalsByKey = (0, util_1.indexBy)(await (0, assemblies_1.allTypeScriptSnippets)(asms), key_1.snippetKey);
    const translations = Object.keys(originalsByKey)
        .map((key) => translationsTablet.tryGetSnippet(key))
        .filter(util_1.isDefined);
    for (const trans of translations) {
        for (const fqn of trans.snippet.fqnsReferenced ?? []) {
            ret.add(fqn, trans);
        }
    }
    return { originalsByKey, translationsByFqn: ret.index };
}
function makeDict(xs) {
    const ret = {};
    for (const [str, a] of xs) {
        ret[str] = a;
    }
    return ret;
}
//# sourceMappingURL=infuse.js.map