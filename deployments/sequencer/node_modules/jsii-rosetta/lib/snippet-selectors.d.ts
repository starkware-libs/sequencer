import { TranslatedSnippet } from './tablets/tablets';
export type SnippetSelector = (snippets: TranslatedSnippet[]) => TranslatedSnippet;
/**
 * Returns the longest available snippet.
 */
export declare function longest(snippets: TranslatedSnippet[]): TranslatedSnippet;
/**
 * Returns the shortest available snippet.
 */
export declare function shortest(snippets: TranslatedSnippet[]): TranslatedSnippet;
/**
 * Returns the snippet with the length closest to the mean length of the available snippets.
 */
export declare function meanLength(snippets: TranslatedSnippet[]): TranslatedSnippet;
/**
 * Finds and returns the mean sparse vector of available snippets for each type.
 */
export declare function mean(snippets: TranslatedSnippet[]): TranslatedSnippet;
//# sourceMappingURL=snippet-selectors.d.ts.map