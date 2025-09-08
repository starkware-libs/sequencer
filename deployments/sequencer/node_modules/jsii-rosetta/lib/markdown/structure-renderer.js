"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.StructureRenderer = void 0;
const markdown_1 = require("./markdown");
/**
 * A renderer that will render a CommonMark tree to show its structure
 */
class StructureRenderer {
    block_quote(node, context) {
        return this.handle('block_quote', node, context);
    }
    code(node, context) {
        return this.handle('code', node, context);
    }
    code_block(node, context) {
        return this.handle('code_block', node, context);
    }
    text(node, context) {
        return this.handle('text', node, context);
    }
    softbreak(node, context) {
        return this.handle('softbreak', node, context);
    }
    linebreak(node, context) {
        return this.handle('linebreak', node, context);
    }
    emph(node, context) {
        return this.handle('emph', node, context);
    }
    strong(node, context) {
        return this.handle('strong', node, context);
    }
    html_inline(node, context) {
        return this.handle('html_inline', node, context);
    }
    html_block(node, context) {
        return this.handle('html_block', node, context);
    }
    link(node, context) {
        return this.handle('link', node, context);
    }
    image(node, context) {
        return this.handle('image', node, context);
    }
    document(node, context) {
        return this.handle('document', node, context);
    }
    paragraph(node, context) {
        return this.handle('paragraph', node, context);
    }
    list(node, context) {
        return this.handle('list', node, context);
    }
    item(node, context) {
        return this.handle('item', node, context);
    }
    heading(node, context) {
        return this.handle('heading', node, context);
    }
    thematic_break(node, context) {
        return this.handle('thematic_break', node, context);
    }
    custom_block(node, context) {
        return this.handle('custom_block', node, context);
    }
    custom_inline(node, context) {
        return this.handle('custom_inline', node, context);
    }
    handle(name, node, context) {
        const contents = context.content();
        const enterText = [name, inspectNode(node)].filter((x) => x).join(' ');
        if (contents) {
            return `(${enterText}\n${(0, markdown_1.prefixLines)('  ', contents)})\n`;
        }
        return `(${enterText})\n`;
    }
}
exports.StructureRenderer = StructureRenderer;
function inspectNode(n) {
    const INTERESTING_KEYS = [
        'literal',
        'destination',
        'title',
        'info',
        'level',
        'listType',
        'listTight',
        'listStart',
        'listDelimiter',
    ];
    const ret = {};
    // tslint:disable-next-line:forin
    for (const key of INTERESTING_KEYS) {
        const value = n[key];
        if (typeof value === 'string' || typeof value === 'number') {
            ret[key] = value;
        }
    }
    return JSON.stringify(ret);
}
//# sourceMappingURL=structure-renderer.js.map