"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JavaDocRenderer = void 0;
const escapes_1 = require("./escapes");
const markdown_renderer_1 = require("./markdown-renderer");
const ESCAPE = (0, escapes_1.makeJavaEscaper)();
/**
 * A renderer that will render a CommonMark tree to JavaDoc comments
 *
 * Mostly concerns itself with code annotations and escaping; tags that the
 * XML formatter doesn't have equivalents for will be rendered back to MarkDown.
 */
class JavaDocRenderer extends markdown_renderer_1.MarkdownRenderer {
    block_quote(_node, context) {
        return `<blockquote>${context.content()}</blockquote>`;
    }
    code(node, _context) {
        return `<code>${ESCAPE.text(node.literal)}</code>`;
    }
    /**
     * Render code blocks for JavaDoc
     *
     * See https://reflectoring.io/howto-format-code-snippets-in-javadoc/
     *
     * Since we need to display @ inside our examples and we don't have to
     * care about writability, the most robust option seems to be <pre>
     * tags with escaping of bad characters.
     */
    code_block(node, _context) {
        return (0, markdown_renderer_1.para)(`<blockquote><pre>\n${ESCAPE.text(node.literal)}</pre></blockquote>`);
    }
    text(node, _context) {
        return ESCAPE.text(node.literal) ?? '';
    }
    link(node, context) {
        return `<a href="${ESCAPE.attribute(node.destination) ?? ''}">${context.content()}</a>`;
    }
    document(_node, context) {
        return (0, markdown_renderer_1.stripTrailingWhitespace)(specialDocCommentEscape(collapseParaJava(context.content())));
    }
    heading(node, context) {
        return (0, markdown_renderer_1.para)(`<h${node.level}>${context.content()}</h${node.level}>`);
    }
    list(node, context) {
        const tag = node.listType === 'bullet' ? 'ul' : 'ol';
        return (0, markdown_renderer_1.para)(`<${tag}>\n${context.content()}</${tag}>`);
    }
    item(_node, context) {
        return `<li>${(0, markdown_renderer_1.stripPara)(context.content())}</li>\n`;
    }
    image(node, context) {
        return `<img alt="${ESCAPE.text2attr(context.content())}" src="${ESCAPE.attribute(node.destination) ?? ''}">`;
    }
    emph(_node, context) {
        return `<em>${context.content()}</em>`;
    }
    strong(_node, context) {
        return `<strong>${context.content()}</strong>`;
    }
    thematic_break(_node, _context) {
        return (0, markdown_renderer_1.para)('<hr>');
    }
}
exports.JavaDocRenderer = JavaDocRenderer;
function collapseParaJava(x) {
    return (0, markdown_renderer_1.collapsePara)(x, '\n<p>\n');
}
/**
 * A final single-pass escape of '* /' which might otherwise end a doc comment.
 *
 * We have to do this in one final pass because I've observed that in running
 * next, the MarkDown parser will parse the two symbols to:
 *
 *    [..., text('*'), text('/'), ...]
 *
 * which means we have no other ability to observe the two-character combination
 * properly.
 */
function specialDocCommentEscape(x) {
    return x.replace(new RegExp('\\*\\/', 'g'), '*&#47;');
}
//# sourceMappingURL=javadoc-renderer.js.map