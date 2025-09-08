"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ReplaceCodeTransform = void 0;
/**
 * Renderer that replaces code blocks in a MarkDown document
 */
class ReplaceCodeTransform {
    constructor(replacer) {
        this.replacer = replacer;
    }
    code_block(node) {
        const line = node.sourcepos[0][0];
        const ret = this.replacer({
            language: node.info ?? '',
            source: node.literal ?? '',
        }, line);
        node.info = ret.language;
        node.literal = ret.source + (!ret.source || ret.source.endsWith('\n') ? '' : '\n');
    }
    block_quote() {
        /* nothing */
    }
    code() {
        /* nothing */
    }
    text() {
        /* nothing */
    }
    softbreak() {
        /* nothing */
    }
    linebreak() {
        /* nothing */
    }
    emph() {
        /* nothing */
    }
    strong() {
        /* nothing */
    }
    html_inline() {
        /* nothing */
    }
    html_block() {
        /* nothing */
    }
    link() {
        /* nothing */
    }
    image() {
        /* nothing */
    }
    document() {
        /* nothing */
    }
    paragraph() {
        /* nothing */
    }
    list() {
        /* nothing */
    }
    item() {
        /* nothing */
    }
    heading() {
        /* nothing */
    }
    thematic_break() {
        /* nothing */
    }
    custom_block() {
        /* nothing */
    }
    custom_inline() {
        /* nothing */
    }
}
exports.ReplaceCodeTransform = ReplaceCodeTransform;
//# sourceMappingURL=replace-code-renderer.js.map