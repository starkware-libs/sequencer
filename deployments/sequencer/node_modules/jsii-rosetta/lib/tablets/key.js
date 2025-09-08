"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.snippetKey = snippetKey;
const crypto = require("node:crypto");
const record_references_version_1 = require("../languages/record-references-version");
const snippet_1 = require("../snippet");
/**
 * Determine the key for a code block
 */
function snippetKey(snippet) {
    const h = crypto.createHash('sha256');
    h.update(String(record_references_version_1.RECORD_REFERENCES_VERSION));
    // Mix in API location to distinguish between similar snippets
    h.update((0, snippet_1.renderApiLocation)(snippet.location.api));
    h.update(':');
    h.update(snippet.visibleSource);
    return h.digest('hex');
}
//# sourceMappingURL=key.js.map