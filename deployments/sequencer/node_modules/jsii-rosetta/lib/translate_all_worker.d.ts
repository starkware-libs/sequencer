import { TypeScriptSnippet } from './snippet';
import { TranslatedSnippetSchema } from './tablets/schema';
import { RosettaDiagnostic } from './translate';
import { TranslateAllResult } from './translate_all';
export interface TranslateBatchRequest {
    readonly snippets: TypeScriptSnippet[];
    readonly includeCompilerDiagnostics: boolean;
}
export interface TranslateBatchResponse {
    readonly translatedSchemas: TranslatedSnippetSchema[];
    readonly diagnostics: RosettaDiagnostic[];
}
/**
 * Translate the given snippets using a single compiler
 */
export declare function singleThreadedTranslateAll(snippets: TypeScriptSnippet[], includeCompilerDiagnostics: boolean): TranslateAllResult;
//# sourceMappingURL=translate_all_worker.d.ts.map