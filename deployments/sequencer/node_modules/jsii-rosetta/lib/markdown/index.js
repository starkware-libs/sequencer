"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.markDownToStructure = markDownToStructure;
exports.markDownToJavaDoc = markDownToJavaDoc;
exports.markDownToXmlDoc = markDownToXmlDoc;
const javadoc_renderer_1 = require("./javadoc-renderer");
const markdown_1 = require("./markdown");
const structure_renderer_1 = require("./structure-renderer");
const xml_comment_renderer_1 = require("./xml-comment-renderer");
/**
 * All the visitors in this module expose CommonMark types in their API
 *
 * We want to keep CommonMark as a private dependency (so we don't have to
 * mark it as peerDependency and can keep its @types in devDependencies),
 * so we re-expose the main functionality needed by pacmak as functions
 * that operate on basic types here.
 */
function markDownToStructure(source) {
    return (0, markdown_1.transformMarkdown)(source, new structure_renderer_1.StructureRenderer());
}
function markDownToJavaDoc(source) {
    return (0, markdown_1.transformMarkdown)(source, new javadoc_renderer_1.JavaDocRenderer());
}
function markDownToXmlDoc(source) {
    return (0, markdown_1.transformMarkdown)(source, new xml_comment_renderer_1.CSharpXmlCommentRenderer());
}
//# sourceMappingURL=index.js.map