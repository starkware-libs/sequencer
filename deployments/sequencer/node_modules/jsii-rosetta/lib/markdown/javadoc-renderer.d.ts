import * as cm from 'commonmark';
import { RendererContext } from './markdown';
import { MarkdownRenderer } from './markdown-renderer';
/**
 * A renderer that will render a CommonMark tree to JavaDoc comments
 *
 * Mostly concerns itself with code annotations and escaping; tags that the
 * XML formatter doesn't have equivalents for will be rendered back to MarkDown.
 */
export declare class JavaDocRenderer extends MarkdownRenderer {
    block_quote(_node: cm.Node, context: RendererContext): string;
    code(node: cm.Node, _context: RendererContext): string;
    /**
     * Render code blocks for JavaDoc
     *
     * See https://reflectoring.io/howto-format-code-snippets-in-javadoc/
     *
     * Since we need to display @ inside our examples and we don't have to
     * care about writability, the most robust option seems to be <pre>
     * tags with escaping of bad characters.
     */
    code_block(node: cm.Node, _context: RendererContext): string;
    text(node: cm.Node, _context: RendererContext): string;
    link(node: cm.Node, context: RendererContext): string;
    document(_node: cm.Node, context: RendererContext): string;
    heading(node: cm.Node, context: RendererContext): string;
    list(node: cm.Node, context: RendererContext): string;
    item(_node: cm.Node, context: RendererContext): string;
    image(node: cm.Node, context: RendererContext): string;
    emph(_node: cm.Node, context: RendererContext): string;
    strong(_node: cm.Node, context: RendererContext): string;
    thematic_break(_node: cm.Node, _context: RendererContext): string;
}
//# sourceMappingURL=javadoc-renderer.d.ts.map