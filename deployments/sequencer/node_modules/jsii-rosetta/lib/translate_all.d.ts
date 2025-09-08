import { TypeScriptSnippet } from './snippet';
import { TranslatedSnippet } from './tablets/tablets';
import { RosettaDiagnostic } from './translate';
/**
 * Divide the work evenly over all processors by running 'translate_all_worker' in Worker Threads, then combine results
 *
 * The workers are fed small queues of work each. We used to divide the entire queue into N
 * but since the work is divided unevenly that led to some workers stopping early, idling while
 * waiting for more work.
 *
 * Never include 'translate_all_worker' directly, only do TypeScript type references (so that in
 * the script we may assume that 'worker_threads' successfully imports).
 */
export declare function translateAll(snippets: TypeScriptSnippet[], includeCompilerDiagnostics: boolean): Promise<TranslateAllResult>;
export interface TranslateAllResult {
    translatedSnippets: TranslatedSnippet[];
    diagnostics: RosettaDiagnostic[];
}
//# sourceMappingURL=translate_all.d.ts.map