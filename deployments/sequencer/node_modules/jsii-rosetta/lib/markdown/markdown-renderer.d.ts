import * as cm from 'commonmark';
import { CommonMarkRenderer, RendererContext } from './markdown';
/**
 * A renderer that will render a CommonMark tree back to MarkDown
 */
export declare class MarkdownRenderer implements CommonMarkRenderer {
    block_quote(_node: cm.Node, context: RendererContext): string;
    code(node: cm.Node, _context: RendererContext): string;
    code_block(node: cm.Node, _context: RendererContext): string;
    text(node: cm.Node, _context: RendererContext): string;
    softbreak(_node: cm.Node, _context: RendererContext): string;
    linebreak(_node: cm.Node, _context: RendererContext): string;
    emph(_node: cm.Node, context: RendererContext): string;
    strong(_node: cm.Node, context: RendererContext): string;
    html_inline(node: cm.Node, _context: RendererContext): string;
    html_block(node: cm.Node, _context: RendererContext): string;
    link(node: cm.Node, context: RendererContext): string;
    image(node: cm.Node, context: RendererContext): string;
    document(_node: cm.Node, context: RendererContext): string;
    paragraph(_node: cm.Node, context: RendererContext): string;
    list(node: cm.Node, context: RendererContext): string;
    item(_node: cm.Node, context: RendererContext): string;
    heading(node: cm.Node, context: RendererContext): string;
    thematic_break(_node: cm.Node, _context: RendererContext): string;
    custom_block(_node: cm.Node, context: RendererContext): string;
    custom_inline(_node: cm.Node, context: RendererContext): string;
}
export declare function para(x: string): string;
/**
 * Collapse paragraph markers
 */
export declare function collapsePara(x: string, brk?: string): string;
/**
 * Strip paragraph markers from start and end
 */
export declare function stripPara(x: string): string;
export declare function stripTrailingWhitespace(x: string): string;
//# sourceMappingURL=markdown-renderer.d.ts.map