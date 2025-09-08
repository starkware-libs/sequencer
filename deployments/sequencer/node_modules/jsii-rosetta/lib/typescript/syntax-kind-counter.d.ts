import * as ts from 'typescript';
import { Spans } from './visible-spans';
export declare class SyntaxKindCounter {
    private readonly visibleSpans;
    private readonly counter;
    constructor(visibleSpans: Spans);
    countKinds(sourceFile: ts.SourceFile): Partial<Record<ts.SyntaxKind, number>>;
    private countNode;
}
//# sourceMappingURL=syntax-kind-counter.d.ts.map