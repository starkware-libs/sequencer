"use strict";
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _Translator_diagnostics;
Object.defineProperty(exports, "__esModule", { value: true });
exports.SnippetTranslator = exports.Translator = void 0;
exports.translateTypeScript = translateTypeScript;
exports.makeRosettaDiagnostic = makeRosettaDiagnostic;
exports.rosettaDiagFromTypescript = rosettaDiagFromTypescript;
const node_util_1 = require("node:util");
const ts = require("typescript");
const languages_1 = require("./languages");
const record_references_1 = require("./languages/record-references");
const target_language_1 = require("./languages/target-language");
const logging = require("./logging");
const o_tree_1 = require("./o-tree");
const renderer_1 = require("./renderer");
const snippet_1 = require("./snippet");
const submodule_reference_1 = require("./submodule-reference");
const key_1 = require("./tablets/key");
const schema_1 = require("./tablets/schema");
const tablets_1 = require("./tablets/tablets");
const syntax_kind_counter_1 = require("./typescript/syntax-kind-counter");
const ts_compiler_1 = require("./typescript/ts-compiler");
const visible_spans_1 = require("./typescript/visible-spans");
const util_1 = require("./util");
function translateTypeScript(source, visitor, options = {}) {
    const translator = new SnippetTranslator({ visibleSource: source.contents, location: { api: { api: 'file', fileName: source.fileName } } }, options);
    const translated = translator.renderUsing(visitor);
    return {
        translation: translated,
        diagnostics: translator.diagnostics.map(rosettaDiagFromTypescript),
    };
}
/**
 * Translate one or more TypeScript snippets into other languages
 *
 * Can be configured to fully typecheck the samples, or perform only syntactical
 * translation.
 */
class Translator {
    constructor(includeCompilerDiagnostics) {
        this.includeCompilerDiagnostics = includeCompilerDiagnostics;
        this.compiler = new ts_compiler_1.TypeScriptCompiler();
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _Translator_diagnostics.set(this, []);
    }
    translate(snip, languages = Object.values(languages_1.TargetLanguage)) {
        logging.debug(`Translating ${(0, key_1.snippetKey)(snip)} ${(0, node_util_1.inspect)(snip.parameters ?? {})}`);
        const translator = this.translatorFor(snip);
        const translations = (0, util_1.mkDict)(languages.flatMap((lang, idx, array) => {
            if (array.slice(0, idx).includes(lang)) {
                // This language was duplicated in the request... we'll skip that here...
                return [];
            }
            const languageConverterFactory = languages_1.TARGET_LANGUAGES[lang];
            const translated = translator.renderUsing(languageConverterFactory.createVisitor());
            return [[lang, { source: translated, version: languageConverterFactory.version }]];
        }));
        if (snip.parameters?.infused === undefined) {
            __classPrivateFieldGet(this, _Translator_diagnostics, "f").push(...translator.diagnostics);
        }
        return tablets_1.TranslatedSnippet.fromSchema({
            translations: {
                ...translations,
                [schema_1.ORIGINAL_SNIPPET_KEY]: { source: snip.visibleSource, version: '0' },
            },
            location: snip.location,
            didCompile: translator.didSuccessfullyCompile,
            fqnsReferenced: translator.fqnsReferenced(),
            fullSource: (0, snippet_1.completeSource)(snip),
            syntaxKindCounter: translator.syntaxKindCounter(),
        });
    }
    get diagnostics() {
        return ts.sortAndDeduplicateDiagnostics(__classPrivateFieldGet(this, _Translator_diagnostics, "f")).map(rosettaDiagFromTypescript);
    }
    /**
     * Return the snippet translator for the given snippet
     *
     * We used to cache these, but each translator holds on to quite a bit of memory,
     * so we don't do that anymore.
     */
    translatorFor(snippet) {
        const translator = new SnippetTranslator(snippet, {
            compiler: this.compiler,
            includeCompilerDiagnostics: this.includeCompilerDiagnostics,
        });
        return translator;
    }
}
exports.Translator = Translator;
_Translator_diagnostics = new WeakMap();
function makeRosettaDiagnostic(isError, formattedMessage) {
    return { isError, formattedMessage, isFromStrictAssembly: false };
}
/**
 * Translate a single TypeScript snippet
 */
class SnippetTranslator {
    constructor(snippet, options = {}) {
        this.options = options;
        this.translateDiagnostics = [];
        this.compileDiagnostics = [];
        const compiler = options.compiler ?? new ts_compiler_1.TypeScriptCompiler();
        const source = (0, snippet_1.completeSource)(snippet);
        const fakeCurrentDirectory = snippet.parameters?.[snippet_1.SnippetParameters.$COMPILATION_DIRECTORY] ?? process.cwd();
        this.compilation = compiler.compileInMemory(removeSlashes((0, snippet_1.formatLocation)(snippet.location)), source, fakeCurrentDirectory);
        // Respect '/// !hide' and '/// !show' directives
        this.visibleSpans = visible_spans_1.Spans.visibleSpansFromSource(source);
        // Find submodule references on explicit imports
        this.submoduleReferences = submodule_reference_1.SubmoduleReference.inSourceFile(this.compilation.rootFile, this.compilation.program.getTypeChecker());
        // This makes it about 5x slower, so only do it on demand
        // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
        this.tryCompile = (options.includeCompilerDiagnostics || snippet.strict) ?? false;
        if (this.tryCompile) {
            const program = this.compilation.program;
            const diagnostics = [
                ...neverThrowing(program.getGlobalDiagnostics)(),
                ...neverThrowing(program.getSyntacticDiagnostics)(this.compilation.rootFile),
                ...neverThrowing(program.getDeclarationDiagnostics)(this.compilation.rootFile),
                ...neverThrowing(program.getSemanticDiagnostics)(this.compilation.rootFile),
            ];
            if (snippet.strict) {
                // In a strict assembly, so we'll need to brand all diagnostics here...
                for (const diag of diagnostics) {
                    (0, util_1.annotateStrictDiagnostic)(diag);
                }
            }
            this.compileDiagnostics.push(...diagnostics);
        }
        /**
         * Intercepts all exceptions thrown by the wrapped call, and logs them to
         * console.error instead of re-throwing, then returns an empty array. This
         * is here to avoid compiler crashes due to broken code examples that cause
         * the TypeScript compiler to hit a "Debug Failure".
         */
        function neverThrowing(call) {
            return (...args) => {
                try {
                    return call(...args);
                }
                catch (err) {
                    const isExpectedTypescriptError = err.message.includes('Debug Failure');
                    if (!isExpectedTypescriptError) {
                        console.error(`Failed to execute ${call.name}: ${err}`);
                    }
                    return [];
                }
            };
        }
    }
    /**
     * Returns a boolean if compilation was attempted, and undefined if it was not.
     */
    get didSuccessfullyCompile() {
        return this.tryCompile ? this.compileDiagnostics.length === 0 : undefined;
    }
    renderUsing(visitor) {
        const converter = new renderer_1.AstRenderer(this.compilation.rootFile, this.compilation.program.getTypeChecker(), visitor, this.options, 
        // If we support transitive submodule access, don't provide a submodule reference map.
        (0, target_language_1.supportsTransitiveSubmoduleAccess)(visitor.language) ? undefined : this.submoduleReferences);
        const converted = converter.convert(this.compilation.rootFile);
        this.translateDiagnostics.push(...filterVisibleDiagnostics(converter.diagnostics, this.visibleSpans));
        return (0, o_tree_1.renderTree)(converted, { indentChar: visitor.indentChar, visibleSpans: this.visibleSpans });
    }
    syntaxKindCounter() {
        const kindCounter = new syntax_kind_counter_1.SyntaxKindCounter(this.visibleSpans);
        return kindCounter.countKinds(this.compilation.rootFile);
    }
    fqnsReferenced() {
        const visitor = new record_references_1.RecordReferencesVisitor(this.visibleSpans);
        const converter = new renderer_1.AstRenderer(this.compilation.rootFile, this.compilation.program.getTypeChecker(), visitor, this.options, this.submoduleReferences);
        converter.convert(this.compilation.rootFile);
        return visitor.fqnsReferenced();
    }
    get diagnostics() {
        return ts.sortAndDeduplicateDiagnostics(this.compileDiagnostics.concat(this.translateDiagnostics));
    }
}
exports.SnippetTranslator = SnippetTranslator;
/**
 * Hide diagnostics that are rosetta-sourced if they are reported against a non-visible span
 */
function filterVisibleDiagnostics(diags, visibleSpans) {
    return diags.filter((d) => d.source !== 'rosetta' || d.start === undefined || visibleSpans.containsPosition(d.start));
}
/**
 * Turn TypeScript diagnostics into Rosetta diagnostics
 */
function rosettaDiagFromTypescript(diag) {
    return {
        isError: diag.category === ts.DiagnosticCategory.Error,
        isFromStrictAssembly: (0, util_1.hasStrictBranding)(diag),
        formattedMessage: ts.formatDiagnosticsWithColorAndContext([diag], DIAG_HOST),
    };
}
const DIAG_HOST = {
    getCurrentDirectory() {
        return '.';
    },
    getCanonicalFileName(fileName) {
        return fileName;
    },
    getNewLine() {
        return '\n';
    },
};
/**
 * Remove slashes from a "where" description, as the TS compiler will interpret it as a directory
 * and we can't have that for compiling literate files
 */
function removeSlashes(x) {
    return x.replace(/\/|\\/g, '.');
}
//# sourceMappingURL=translate.js.map