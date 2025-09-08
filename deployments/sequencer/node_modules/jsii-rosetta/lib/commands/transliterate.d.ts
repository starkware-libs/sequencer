import { TargetLanguage } from '../languages';
import { UnknownSnippetMode } from '../rosetta-reader';
export interface TransliterateAssemblyOptions {
    /**
     * Whether to ignore any missing fixture files or literate markdown documents
     * referenced by the assembly, instead of failing.
     *
     * @default false
     */
    readonly loose?: boolean;
    /**
     * Whether transliteration should fail upon failing to compile an example that
     * required live transliteration.
     *
     * @default false
     */
    readonly strict?: boolean;
    /**
     * A pre-build translation tablet (as produced by `jsii-rosetta extract`).
     *
     * @default - Only the default tablet (`.jsii.tabl.json`) files will be used.
     */
    readonly tablet?: string;
    /**
     * A directory to output translated assemblies to
     *
     * @default - assembly location
     */
    readonly outdir?: string;
    /**
     * Whether or not to live-convert samples
     *
     * @default UnknownSnippetMode.FAIL
     */
    readonly unknownSnippets?: UnknownSnippetMode;
}
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
export declare function transliterateAssembly(assemblyLocations: readonly string[], targetLanguages: readonly TargetLanguage[], options?: TransliterateAssemblyOptions): Promise<void>;
//# sourceMappingURL=transliterate.d.ts.map