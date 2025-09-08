"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.CSharpXmlCommentRenderer = void 0;
const xmldom_1 = require("@xmldom/xmldom");
const escapes_1 = require("./escapes");
const markdown_1 = require("./markdown");
const markdown_renderer_1 = require("./markdown-renderer");
const ESCAPE = (0, escapes_1.makeXmlEscaper)();
/**
 * A renderer that will render a CommonMark tree to .NET XML comments
 *
 * Mostly concerns itself with code annotations and escaping; tags that the
 * XML formatter doesn't have equivalents for will be rendered back to MarkDown.
 */
class CSharpXmlCommentRenderer extends markdown_renderer_1.MarkdownRenderer {
    block_quote(_node, context) {
        return (0, markdown_renderer_1.para)((0, markdown_1.prefixLines)('    ', (0, markdown_renderer_1.stripPara)(context.content())));
    }
    code(node, _context) {
        return `<c>${ESCAPE.text(node.literal)}</c>`;
    }
    code_block(node, _context) {
        return (0, markdown_renderer_1.para)(`<code><![CDATA[\n${node.literal}]]></code>`);
    }
    text(node, _context) {
        return ESCAPE.text(node.literal) ?? '';
    }
    link(node, context) {
        return `<a href="${ESCAPE.attribute(node.destination) ?? ''}">${context.content()}</a>`;
    }
    image(node, context) {
        return `<img alt="${ESCAPE.text2attr(context.content())}" src="${ESCAPE.attribute(node.destination) ?? ''}" />`;
    }
    emph(_node, context) {
        return `<em>${context.content()}</em>`;
    }
    strong(_node, context) {
        return `<strong>${context.content()}</strong>`;
    }
    heading(node, context) {
        return (0, markdown_renderer_1.para)(`<h${node.level}>${context.content()}</h${node.level}>`);
    }
    list(node, context) {
        const listType = node.listType === 'bullet' ? 'bullet' : 'number';
        return (0, markdown_renderer_1.para)(`<list type="${listType}">\n${context.content()}</list>`);
    }
    item(_node, context) {
        return `<description>${(0, markdown_renderer_1.stripPara)(context.content())}</description>\n`;
    }
    thematic_break(_node, _context) {
        return (0, markdown_renderer_1.para)('<hr />');
    }
    /**
     * HTML needs to be converted to XML
     *
     * If we don't do this, the parser will reject the whole XML block once it sees an unclosed
     * <img> tag.
     */
    html_inline(node, _context) {
        const html = node.literal ?? '';
        try {
            // An html string fails to parse unless it is wrapped into a document root element
            // We fake this, by wrapping the inline html into an artificial root element,
            // and for rendering only selecting its children.
            const dom = new xmldom_1.DOMParser().parseFromString(`<jsii-root>${html}</jsii-root>`, xmldom_1.MIME_TYPE.HTML);
            const fragment = dom.createDocumentFragment();
            for (const child of Array.from(dom.firstChild?.childNodes ?? [])) {
                fragment.appendChild(child);
            }
            return new xmldom_1.XMLSerializer().serializeToString(fragment);
        }
        catch {
            // Could not parse - we'll escape unsafe XML entities here...
            return html.replace(/[<>&]/g, (char) => {
                switch (char) {
                    case '&':
                        return '&amp;';
                    case '<':
                        return '&lt;';
                    case '>':
                        return '&gt;';
                    default:
                        return char;
                }
            });
        }
    }
    /**
     * HTML needs to be converted to XML
     *
     * If we don't do this, the parser will reject the whole XML block once it sees an unclosed
     * <img> tag.
     */
    html_block(node, context) {
        return this.html_inline(node, context);
    }
}
exports.CSharpXmlCommentRenderer = CSharpXmlCommentRenderer;
//# sourceMappingURL=xml-comment-renderer.js.map