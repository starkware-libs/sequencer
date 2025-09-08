"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Rosetta = exports.RosettaTabletReader = exports.UnknownSnippetMode = void 0;
const assemblies_1 = require("./jsii/assemblies");
const logging = require("./logging");
const markdown_1 = require("./markdown/markdown");
const markdown_renderer_1 = require("./markdown/markdown-renderer");
const replace_typescript_transform_1 = require("./markdown/replace-typescript-transform");
const snippet_1 = require("./snippet");
const key_1 = require("./tablets/key");
const tablets_1 = require("./tablets/tablets");
const translate_1 = require("./translate");
const util_1 = require("./util");
var UnknownSnippetMode;
(function (UnknownSnippetMode) {
    /**
     * Return the snippet as given (untranslated)
     */
    UnknownSnippetMode["VERBATIM"] = "verbatim";
    /**
     * Live-translate the snippet as best as we can
     */
    UnknownSnippetMode["TRANSLATE"] = "translate";
    /**
     * Throw an error if this occurs
     */
    UnknownSnippetMode["FAIL"] = "fail";
})(UnknownSnippetMode || (exports.UnknownSnippetMode = UnknownSnippetMode = {}));
/**
 * Entry point class for consumers of Rosetta tablets (primarily: pacmak)
 *
 * Rosetta can work in one of two modes:
 *
 * 1. Live translation of snippets.
 * 2. Read translations from a pre-translated tablet (prepared using `jsii-rosetta extract` command).
 *
 * The second method affords more control over the precise circumstances of
 * sample compilation and is recommended, but the first method will do
 * when the second one is not necessary.
 */
class RosettaTabletReader {
    constructor(options = {}) {
        this.options = options;
        /**
         * Newly translated samples
         *
         * In case live translation has been enabled, all samples that have been translated on-the-fly
         * are added to this tablet.
         */
        this.liveTablet = new tablets_1.LanguageTablet();
        this.loadedTablets = [];
        this.extractedSnippets = new Map();
        this.loose = !!options.loose;
        this.unknownSnippets = options.unknownSnippets ?? UnknownSnippetMode.VERBATIM;
        this.translator = new translate_1.Translator(options.includeCompilerDiagnostics ?? false);
        this._prefixDisclaimer = options.prefixDisclaimer ?? false;
    }
    /**
     * Diagnostics encountered while doing live translation
     */
    get diagnostics() {
        return this.translator.diagnostics;
    }
    /**
     * Load a tablet as a source for translateable snippets
     *
     * Note: the snippets loaded from this tablet will NOT be validated for
     * their fingerprints or translator versions! If a matching snippet is found
     * in the tablet, it will always be returned, whether or not it is stale.
     */
    async loadTabletFromFile(tabletFile) {
        const tablet = new tablets_1.LanguageTablet();
        await tablet.load(tabletFile);
        this.addTablet(tablet);
    }
    /**
     * Directly add a tablet to the list of tablets to load translations from
     */
    addTablet(tablet) {
        this.loadedTablets.push(tablet);
    }
    /**
     * Add an assembly
     *
     * If a default tablet file is found in the assembly's directory, it will be
     * loaded (and assumed to contain a complete list of translated snippets for
     * this assembly already).
     *
     * Otherwise, if live conversion is enabled, the snippets in the assembly
     * become available for live translation later. This is necessary because we probably
     * need to fixturize snippets for successful compilation, and the information
     * pacmak sends our way later on is not going to be enough to do that.
     */
    async addAssembly(assembly, assemblyDir) {
        const defaultTablet = (0, assemblies_1.guessTabletLocation)(assemblyDir);
        if (await (0, util_1.pathExists)(defaultTablet)) {
            try {
                await this.loadTabletFromFile(defaultTablet);
                return;
            }
            catch (e) {
                logging.warn(`Error loading ${defaultTablet}: ${e.message}. Skipped.`);
            }
        }
        // Inventarize the snippets from this assembly, but only if there's a chance
        // we're going to need them.
        if (this.unknownSnippets === UnknownSnippetMode.TRANSLATE) {
            for (const tsnip of await (0, assemblies_1.allTypeScriptSnippets)([{ assembly, directory: assemblyDir }], this.loose)) {
                this.extractedSnippets.set((0, key_1.snippetKey)(tsnip), tsnip);
            }
        }
    }
    /**
     * Translate the given snippet for the given target language
     *
     * This will either:
     *
     * - Find an existing translation in a tablet and return that, if available.
     * - Otherwise, find a fixturized version of this snippet in an assembly that
     *   was loaded beforehand, and translate it on-the-fly. Finding the fixture
     *   will be based on the snippet key, which consists of a hash of the
     *   visible source and the API location.
     * - Otherwise, translate the snippet as-is (without fixture information).
     *
     * This will do and store a full conversion of the given snippet, even if it only
     * returns one language. Subsequent retrievals for the same snippet in other
     * languages will reuse the translation from cache.
     *
     * If you are calling this for the side effect of adding translations to the live
     * tablet, you only need to do that for one language.
     */
    translateSnippet(source, targetLang) {
        // Look for it in loaded tablets (or previous conversions)
        for (const tab of this.allTablets) {
            const ret = tab.lookup(source, targetLang);
            if (ret !== undefined) {
                return this.prefixDisclaimer(ret, this._prefixDisclaimer);
            }
        }
        if (this.unknownSnippets === UnknownSnippetMode.VERBATIM) {
            return this.prefixDisclaimer({
                language: targetLang,
                source: source.visibleSource,
            }, this._prefixDisclaimer);
        }
        if (this.unknownSnippets === UnknownSnippetMode.FAIL) {
            const message = [
                'The following snippet was not found in any of the loaded tablets:',
                source.visibleSource,
                `Location: ${JSON.stringify(source.location)}`,
                `Language: ${targetLang}`,
            ].join('\n');
            throw new Error(message);
        }
        if (this.options.targetLanguages && !this.options.targetLanguages.includes(targetLang)) {
            throw new Error(`Rosetta configured for live conversion to ${this.options.targetLanguages.join(', ')}, but requested ${targetLang}`);
        }
        // See if we can find a fixturized version of this snippet. If so, use that do the live
        // conversion.
        const extracted = this.extractedSnippets.get((0, key_1.snippetKey)(source));
        if (extracted !== undefined) {
            const snippet = this.translator.translate(extracted, this.options.targetLanguages);
            this.liveTablet.addSnippet(snippet);
            return this.prefixDisclaimer(snippet.get(targetLang), this._prefixDisclaimer);
        }
        // Try to live-convert it as-is.
        const snippet = this.translator.translate(source, this.options.targetLanguages);
        this.liveTablet.addSnippet(snippet);
        return this.prefixDisclaimer(snippet.get(targetLang), this._prefixDisclaimer);
    }
    /**
     * Translate a snippet found in the "@ example" section of a jsii assembly
     *
     * Behaves exactly like `translateSnippet`, so see that method for documentation.
     */
    translateExample(apiLocation, example, targetLang, strict, compileDirectory = process.cwd()) {
        const location = { api: apiLocation, field: { field: 'example' } };
        const snippet = (0, snippet_1.typeScriptSnippetFromSource)(example, location, strict, {
            [snippet_1.SnippetParameters.$COMPILATION_DIRECTORY]: compileDirectory,
        });
        const translated = this.translateSnippet(snippet, targetLang);
        return translated ?? { language: 'typescript', source: example };
    }
    /**
     * Translate all TypeScript snippets found in a block of Markdown text
     *
     * For each snippet, behaves exactly like `translateSnippet`, so see that
     * method for documentation.
     */
    translateSnippetsInMarkdown(apiLocation, markdown, targetLang, strict, translationToCodeBlock = id, compileDirectory = process.cwd()) {
        return (0, markdown_1.transformMarkdown)(markdown, new markdown_renderer_1.MarkdownRenderer(), new replace_typescript_transform_1.ReplaceTypeScriptTransform(apiLocation, strict, (tsSnip) => {
            const translated = this.translateSnippet((0, snippet_1.updateParameters)(tsSnip, {
                [snippet_1.SnippetParameters.$COMPILATION_DIRECTORY]: compileDirectory,
            }), targetLang);
            if (!translated) {
                return undefined;
            }
            return translationToCodeBlock(translated);
        }));
    }
    printDiagnostics(stream, colors = true) {
        (0, util_1.printDiagnostics)(this.diagnostics, stream, colors);
    }
    get hasErrors() {
        return this.diagnostics.some((d) => d.isError);
    }
    get allTablets() {
        return [...this.loadedTablets, this.liveTablet];
    }
    /**
     * Adds a disclaimer to the front of the example if the prefixDisclaimer
     * flag is set and we know it does not compile.
     */
    prefixDisclaimer(translation, prefixDisclaimer) {
        if (!prefixDisclaimer || translation?.didCompile !== false) {
            return translation;
        }
        const comment = (0, util_1.commentToken)(translation.language);
        const disclaimer = 'Example automatically generated from non-compiling source. May contain errors.';
        return {
            ...translation,
            source: `${comment} ${disclaimer}\n${translation.source}`,
        };
    }
}
exports.RosettaTabletReader = RosettaTabletReader;
function id(x) {
    return x;
}
/**
 * Backwards compatibility
 *
 * @deprecated use RosettaTabletReader instead
 */
class Rosetta extends RosettaTabletReader {
}
exports.Rosetta = Rosetta;
//# sourceMappingURL=rosetta-reader.js.map