"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ReplaceTypeScriptTransform = void 0;
const replace_code_renderer_1 = require("./replace-code-renderer");
const snippet_1 = require("../snippet");
/**
 * A specialization of ReplaceCodeTransform that maintains state about TypeScript snippets
 */
class ReplaceTypeScriptTransform extends replace_code_renderer_1.ReplaceCodeTransform {
    constructor(api, strict, replacer) {
        super((block, line) => {
            const languageParts = block.language ? block.language.split(' ') : [];
            if (languageParts[0] !== 'typescript' && languageParts[0] !== 'ts') {
                return block;
            }
            const tsSnippet = (0, snippet_1.typeScriptSnippetFromSource)(block.source, { api, field: { field: 'markdown', line } }, strict, (0, snippet_1.parseKeyValueList)(languageParts.slice(1)));
            return replacer(tsSnippet) ?? block;
        });
    }
}
exports.ReplaceTypeScriptTransform = ReplaceTypeScriptTransform;
//# sourceMappingURL=replace-typescript-transform.js.map