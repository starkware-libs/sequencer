import * as spec from '@jsii/spec';
import { TargetLanguage } from './languages';
import { CodeBlock } from './markdown/types';
import { TypeScriptSnippet, ApiLocation } from './snippet';
import { LanguageTablet, Translation } from './tablets/tablets';
export declare enum UnknownSnippetMode {
    /**
     * Return the snippet as given (untranslated)
     */
    VERBATIM = "verbatim",
    /**
     * Live-translate the snippet as best as we can
     */
    TRANSLATE = "translate",
    /**
     * Throw an error if this occurs
     */
    FAIL = "fail"
}
export interface RosettaOptions {
    /**
     * Whether or not to live-convert samples
     *
     * @default UnknownSnippetMode.VERBATIM
     */
    readonly unknownSnippets?: UnknownSnippetMode;
    /**
     * Target languages to use for live conversion
     *
     * @default All languages
     */
    readonly targetLanguages?: readonly TargetLanguage[];
    /**
     * Whether to include compiler diagnostics in the compilation results.
     */
    readonly includeCompilerDiagnostics?: boolean;
    /**
     * Whether this Rosetta should operate in "loose" mode, where missing literate
     * source files and missing fixtures are ignored instead of failing.
     *
     * @default false
     */
    readonly loose?: boolean;
    /**
     * Adds a disclaimer to start of snippet if it did not compile.
     *
     * @default false
     */
    readonly prefixDisclaimer?: boolean;
}
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
export declare class RosettaTabletReader {
    private readonly options;
    /**
     * Newly translated samples
     *
     * In case live translation has been enabled, all samples that have been translated on-the-fly
     * are added to this tablet.
     */
    readonly liveTablet: LanguageTablet;
    private readonly loadedTablets;
    private readonly extractedSnippets;
    private readonly translator;
    private readonly loose;
    private readonly unknownSnippets;
    private readonly _prefixDisclaimer;
    constructor(options?: RosettaOptions);
    /**
     * Diagnostics encountered while doing live translation
     */
    get diagnostics(): readonly import("./translate").RosettaDiagnostic[];
    /**
     * Load a tablet as a source for translateable snippets
     *
     * Note: the snippets loaded from this tablet will NOT be validated for
     * their fingerprints or translator versions! If a matching snippet is found
     * in the tablet, it will always be returned, whether or not it is stale.
     */
    loadTabletFromFile(tabletFile: string): Promise<void>;
    /**
     * Directly add a tablet to the list of tablets to load translations from
     */
    addTablet(tablet: LanguageTablet): void;
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
    addAssembly(assembly: spec.Assembly, assemblyDir: string): Promise<void>;
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
    translateSnippet(source: TypeScriptSnippet, targetLang: TargetLanguage): Translation | undefined;
    /**
     * Translate a snippet found in the "@ example" section of a jsii assembly
     *
     * Behaves exactly like `translateSnippet`, so see that method for documentation.
     */
    translateExample(apiLocation: ApiLocation, example: string, targetLang: TargetLanguage, strict: boolean, compileDirectory?: string): Translation;
    /**
     * Translate all TypeScript snippets found in a block of Markdown text
     *
     * For each snippet, behaves exactly like `translateSnippet`, so see that
     * method for documentation.
     */
    translateSnippetsInMarkdown(apiLocation: ApiLocation, markdown: string, targetLang: TargetLanguage, strict: boolean, translationToCodeBlock?: (x: Translation) => CodeBlock, compileDirectory?: string): string;
    printDiagnostics(stream: NodeJS.WritableStream, colors?: boolean): void;
    get hasErrors(): boolean;
    private get allTablets();
    /**
     * Adds a disclaimer to the front of the example if the prefixDisclaimer
     * flag is set and we know it does not compile.
     */
    private prefixDisclaimer;
}
/**
 * Backwards compatibility
 *
 * @deprecated use RosettaTabletReader instead
 */
export declare class Rosetta extends RosettaTabletReader {
}
//# sourceMappingURL=rosetta-reader.d.ts.map