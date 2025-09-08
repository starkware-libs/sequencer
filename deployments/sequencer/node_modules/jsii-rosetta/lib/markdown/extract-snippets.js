"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.extractTypescriptSnippetsFromMarkdown = extractTypescriptSnippetsFromMarkdown;
const cm = require("commonmark");
const replace_typescript_transform_1 = require("./replace-typescript-transform");
const markdown_1 = require("../markdown/markdown");
function extractTypescriptSnippetsFromMarkdown(markdown, location, strict) {
    const parser = new cm.Parser();
    const doc = parser.parse(markdown);
    const ret = [];
    (0, markdown_1.visitCommonMarkTree)(doc, new replace_typescript_transform_1.ReplaceTypeScriptTransform(location, strict, (ts) => {
        ret.push(ts);
        return undefined;
    }));
    return ret;
}
//# sourceMappingURL=extract-snippets.js.map