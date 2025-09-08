"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.longest = longest;
exports.shortest = shortest;
exports.meanLength = meanLength;
exports.mean = mean;
class SnippetScore {
    constructor(snippet, score) {
        this.snippet = snippet;
        this.score = score;
    }
}
/**
 * Returns the longest available snippet.
 */
function longest(snippets) {
    if (snippets.length === 0) {
        throw new Error('longest: array cannot be empty');
    }
    const snippetScores = [];
    for (const snippet of snippets) {
        snippetScores.push({ snippet: snippet, score: snippet.originalSource.source.length });
    }
    return getMaxScore(snippetScores).snippet;
}
/**
 * Returns the shortest available snippet.
 */
function shortest(snippets) {
    if (snippets.length === 0) {
        throw new Error('shortest: array cannot be empty');
    }
    const snippetScores = [];
    for (const snippet of snippets) {
        snippetScores.push({ snippet: snippet, score: snippet.originalSource.source.length });
    }
    return getMinScore(snippetScores).snippet;
}
/**
 * Returns the snippet with the length closest to the mean length of the available snippets.
 */
function meanLength(snippets) {
    if (snippets.length === 0) {
        throw new Error('meanLength: array cannot be empty');
    }
    const meanLen = snippets.reduce((x, y) => x + y.originalSource.source.length, 0) / snippets.length;
    const snippetScores = [];
    for (const snippet of snippets) {
        snippetScores.push({ snippet: snippet, score: Math.abs(snippet.originalSource.source.length - meanLen) });
    }
    return getMinScore(snippetScores).snippet;
}
/**
 * Finds and returns the mean sparse vector of available snippets for each type.
 */
function mean(snippets) {
    if (snippets.length === 0) {
        throw new Error('mean: array cannot be empty');
    }
    // Find mean counter.
    const counters = [];
    snippets.map((snippet) => {
        counters.push(snippet.snippet.syntaxKindCounter ?? {});
    });
    const meanCounter = findCenter(counters);
    // Find counter with closest euclidian distance.
    const snippetScores = [];
    for (let i = 0; i < snippets.length; i++) {
        snippetScores.push({ snippet: snippets[i], score: euclideanDistance(meanCounter, counters[i]) });
    }
    return getMinScore(snippetScores).snippet;
}
/**
 * Given a list of Records, outputs a Record that averages all the items in each Record.
 */
function findCenter(counters) {
    const centerCounter = {};
    for (const counter of counters) {
        for (const [key, value] of Object.entries(counter)) {
            centerCounter[key] = value + (centerCounter[key] ?? 0);
        }
    }
    const total = counters.length;
    Object.entries(centerCounter).map(([key, value]) => {
        centerCounter[key] = value / total;
    });
    return centerCounter;
}
/**
 * Finds the euclidean distance between two sparse vectors.
 * !!! This function assumes that the center parameter is a superset of the counter parameter. !!!
 */
function euclideanDistance(center, counter) {
    const individualDistances = [];
    Object.entries(center).map(([key, value]) => {
        individualDistances.push(value - (counter[key] ?? 0));
    });
    return individualDistances.reduce((acc, curr) => acc + Math.sqrt(Math.pow(curr, 2)), 0);
}
function getMaxScore(snippetScores) {
    return snippetScores.reduce((x, y) => {
        return x.score >= y.score ? x : y;
    });
}
function getMinScore(snippetScores) {
    return snippetScores.reduce((x, y) => {
        return x.score <= y.score ? x : y;
    });
}
//# sourceMappingURL=snippet-selectors.js.map