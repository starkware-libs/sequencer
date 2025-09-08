"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.translateAll = translateAll;
const os = require("node:os");
const path = require("node:path");
const workerpool = require("workerpool");
const logging = require("./logging");
const tablets_1 = require("./tablets/tablets");
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
async function translateAll(snippets, includeCompilerDiagnostics) {
    // Use about half the advertised cores because hyperthreading doesn't seem to
    // help that much, or we become I/O-bound at some point. On my machine, using
    // more than half the cores actually makes it slower.
    // Cap to a reasonable top-level limit to prevent thrash on machines with many, many cores.
    const N = process.env.JSII_ROSETTA_MAX_WORKER_COUNT
        ? parseInt(process.env.JSII_ROSETTA_MAX_WORKER_COUNT)
        : Math.min(16, Math.max(1, Math.ceil(os.cpus().length / 2)));
    const snippetArr = Array.from(snippets);
    logging.info(`Translating ${snippetArr.length} snippets using ${N} workers`);
    const pool = workerpool.pool(path.join(__dirname, 'translate_all_worker.js'), {
        maxWorkers: N,
    });
    try {
        const requests = batchSnippets(snippetArr, includeCompilerDiagnostics);
        const responses = await Promise.all(requests.map((request) => pool.exec('translateBatch', [request])));
        const diagnostics = new Array();
        const translatedSnippets = new Array();
        // Combine results
        for (const response of responses) {
            diagnostics.push(...response.diagnostics);
            translatedSnippets.push(...response.translatedSchemas.map(tablets_1.TranslatedSnippet.fromSchema));
        }
        return { diagnostics, translatedSnippets };
    }
    finally {
        // Not waiting on purpose
        void pool.terminate();
    }
}
function batchSnippets(snippets, includeCompilerDiagnostics, batchSize = 10) {
    const ret = [];
    for (let i = 0; i < snippets.length; i += batchSize) {
        ret.push({
            snippets: snippets.slice(i, i + batchSize),
            includeCompilerDiagnostics,
        });
    }
    return ret;
}
//# sourceMappingURL=translate_all.js.map