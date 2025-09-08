import * as cm from 'commonmark';
import { CommonMarkRenderer, RendererContext } from './markdown';
/**
 * A renderer that will render a CommonMark tree to show its structure
 */
export declare class StructureRenderer implements CommonMarkRenderer {
    block_quote(node: cm.Node, context: RendererContext): string;
    code(node: cm.Node, context: RendererContext): string;
    code_block(node: cm.Node, context: RendererContext): string;
    text(node: cm.Node, context: RendererContext): string;
    softbreak(node: cm.Node, context: RendererContext): string;
    linebreak(node: cm.Node, context: RendererContext): string;
    emph(node: cm.Node, context: RendererContext): string;
    strong(node: cm.Node, context: RendererContext): string;
    html_inline(node: cm.Node, context: RendererContext): string;
    html_block(node: cm.Node, context: RendererContext): string;
    link(node: cm.Node, context: RendererContext): string;
    image(node: cm.Node, context: RendererContext): string;
    document(node: cm.Node, context: RendererContext): string;
    paragraph(node: cm.Node, context: RendererContext): string;
    list(node: cm.Node, context: RendererContext): string;
    item(node: cm.Node, context: RendererContext): string;
    heading(node: cm.Node, context: RendererContext): string;
    thematic_break(node: cm.Node, context: RendererContext): string;
    custom_block(node: cm.Node, context: RendererContext): string;
    custom_inline(node: cm.Node, context: RendererContext): string;
    private handle;
}
//# sourceMappingURL=structure-renderer.d.ts.map