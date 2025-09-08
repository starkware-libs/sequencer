"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.MarkdownRenderer = void 0;
exports.para = para;
exports.collapsePara = collapsePara;
exports.stripPara = stripPara;
exports.stripTrailingWhitespace = stripTrailingWhitespace;
const markdown_1 = require("./markdown");
/**
 * A renderer that will render a CommonMark tree back to MarkDown
 */
class MarkdownRenderer {
    block_quote(_node, context) {
        return para((0, markdown_1.prefixLines)('> ', collapsePara(context.content())));
    }
    code(node, _context) {
        return `\`${node.literal}\``;
    }
    code_block(node, _context) {
        return para(`\`\`\`${node.info ?? ''}\n${node.literal}\`\`\``);
    }
    text(node, _context) {
        return node.literal ?? '';
    }
    softbreak(_node, _context) {
        return '\n';
    }
    linebreak(_node, _context) {
        return '\\\n';
    }
    emph(_node, context) {
        return `*${context.content()}*`;
    }
    strong(_node, context) {
        return `**${context.content()}**`;
    }
    html_inline(node, _context) {
        return node.literal ?? '';
    }
    html_block(node, _context) {
        return node.literal ?? '';
    }
    link(node, context) {
        return `[${context.content()}](${node.destination ?? ''})`;
    }
    image(node, context) {
        return `![${context.content()}](${node.destination ?? ''})`;
    }
    document(_node, context) {
        return stripTrailingWhitespace(collapsePara(context.content()));
    }
    paragraph(_node, context) {
        return para(context.content());
    }
    list(node, context) {
        // A list is not wrapped in a paragraph, but items may contain paragraphs.
        // All elements of a list are definitely 'item's.
        const items = [];
        let i = 1;
        for (const item of (0, markdown_1.cmNodeChildren)(node)) {
            const firstLinePrefix = determineItemPrefix(node, i);
            const hangingPrefix = ' '.repeat(firstLinePrefix.length);
            const rendered = context.recurse(item);
            // Prefix the first line with a different text than subsequent lines
            const prefixed = firstLinePrefix + (0, markdown_1.prefixLines)(hangingPrefix, rendered).slice(hangingPrefix.length);
            items.push(prefixed);
            i += 1;
        }
        return para(items.join('\n'));
    }
    item(_node, context) {
        return collapsePara(context.content());
    }
    heading(node, context) {
        return para(`${'#'.repeat(node.level)} ${context.content()}`);
    }
    thematic_break(_node, _context) {
        return '---\n';
    }
    custom_block(_node, context) {
        return `<custom>${context.content()}</custom>`;
    }
    custom_inline(_node, context) {
        return `<custom>${context.content()}</custom>`;
    }
}
exports.MarkdownRenderer = MarkdownRenderer;
const PARA_BREAK = '\u001d';
function para(x) {
    return `${PARA_BREAK}${x}${PARA_BREAK}`;
}
/**
 * Collapse paragraph markers
 */
function collapsePara(x, brk = '\n\n') {
    /* eslint-disable no-control-regex */
    return x
        .replace(/^\u001d+/, '')
        .replace(/\u001d+$/, '')
        .replace(/\u001d+/g, brk);
    /* eslint-enable no-control-regex */
}
/**
 * Strip paragraph markers from start and end
 */
function stripPara(x) {
    /* eslint-disable-next-line no-control-regex */
    return x.replace(/^\u001d+/, '').replace(/\u001d+$/, '');
}
function determineItemPrefix(listNode, index) {
    if (listNode.listType === 'bullet') {
        return '* ';
    }
    return `${index}${listNode.listDelimiter} `;
}
function stripTrailingWhitespace(x) {
    return x.replace(/[ \t]+$/gm, '');
}
//# sourceMappingURL=markdown-renderer.js.map