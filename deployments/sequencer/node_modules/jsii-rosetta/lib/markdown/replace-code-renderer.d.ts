import * as cm from 'commonmark';
import { CommonMarkVisitor } from './markdown';
import { CodeBlock } from './types';
export type CodeReplacer = (code: CodeBlock, line: number) => CodeBlock;
/**
 * Renderer that replaces code blocks in a MarkDown document
 */
export declare class ReplaceCodeTransform implements CommonMarkVisitor {
    private readonly replacer;
    constructor(replacer: CodeReplacer);
    code_block(node: cm.Node): void;
    block_quote(): void;
    code(): void;
    text(): void;
    softbreak(): void;
    linebreak(): void;
    emph(): void;
    strong(): void;
    html_inline(): void;
    html_block(): void;
    link(): void;
    image(): void;
    document(): void;
    paragraph(): void;
    list(): void;
    item(): void;
    heading(): void;
    thematic_break(): void;
    custom_block(): void;
    custom_inline(): void;
}
//# sourceMappingURL=replace-code-renderer.d.ts.map