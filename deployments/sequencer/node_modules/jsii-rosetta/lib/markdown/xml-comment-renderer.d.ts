import * as cm from 'commonmark';
import { RendererContext } from './markdown';
import { MarkdownRenderer } from './markdown-renderer';
/**
 * A renderer that will render a CommonMark tree to .NET XML comments
 *
 * Mostly concerns itself with code annotations and escaping; tags that the
 * XML formatter doesn't have equivalents for will be rendered back to MarkDown.
 */
export declare class CSharpXmlCommentRenderer extends MarkdownRenderer {
    block_quote(_node: cm.Node, context: RendererContext): string;
    code(node: cm.Node, _context: RendererContext): string;
    code_block(node: cm.Node, _context: RendererContext): string;
    text(node: cm.Node, _context: RendererContext): string;
    link(node: cm.Node, context: RendererContext): string;
    image(node: cm.Node, context: RendererContext): string;
    emph(_node: cm.Node, context: RendererContext): string;
    strong(_node: cm.Node, context: RendererContext): string;
    heading(node: cm.Node, context: RendererContext): string;
    list(node: cm.Node, context: RendererContext): string;
    item(_node: cm.Node, context: RendererContext): string;
    thematic_break(_node: cm.Node, _context: RendererContext): string;
    /**
     * HTML needs to be converted to XML
     *
     * If we don't do this, the parser will reject the whole XML block once it sees an unclosed
     * <img> tag.
     */
    html_inline(node: cm.Node, _context: RendererContext): string;
    /**
     * HTML needs to be converted to XML
     *
     * If we don't do this, the parser will reject the whole XML block once it sees an unclosed
     * <img> tag.
     */
    html_block(node: cm.Node, context: RendererContext): string;
}
//# sourceMappingURL=xml-comment-renderer.d.ts.map