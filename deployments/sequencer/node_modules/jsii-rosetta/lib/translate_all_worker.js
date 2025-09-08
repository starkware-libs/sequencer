"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.singleThreadedTranslateAll = singleThreadedTranslateAll;
/**
 * Pool worker for extract.ts
 */
const workerpool = require("workerpool");
const translate_1 = require("./translate");
function translateBatch(request) {
    const result = singleThreadedTranslateAll(request.snippets, request.includeCompilerDiagnostics);
    return {
        translatedSchemas: result.translatedSnippets.map((s) => s.snippet),
        diagnostics: result.diagnostics,
    };
}
/**
 * Translate the given snippets using a single compiler
 */
function singleThreadedTranslateAll(snippets, includeCompilerDiagnostics) {
    const translatedSnippets = new Array();
    const failures = new Array();
    const translator = new translate_1.Translator(includeCompilerDiagnostics);
    for (const block of snippets) {
        try {
            translatedSnippets.push(translator.translate(block));
        }
        catch (e) {
            failures.push((0, translate_1.makeRosettaDiagnostic)(true, `rosetta: error translating snippet: ${e}\n${e.stack}\n${block.completeSource}`));
        }
    }
    return {
        translatedSnippets,
        diagnostics: [...translator.diagnostics, ...failures],
    };
}
workerpool.worker({ translateBatch });
//# sourceMappingURL=translate_all_worker.js.map