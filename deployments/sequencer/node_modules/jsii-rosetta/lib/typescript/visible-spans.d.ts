import * as ts from 'typescript';
import { Span } from '../o-tree';
/**
 * A class representing a set of non-overlapping Spans.
 */
export declare class Spans {
    private readonly _spans;
    /**
     * Derive visible spans from marked source (`/// !show` and `/// !hide` directives).
     */
    static visibleSpansFromSource(source: string): Spans;
    constructor(_spans: Span[]);
    get spans(): readonly Span[];
    /**
     * Whether another span is fully contained within this set of spans
     */
    fullyContainsSpan(span: Span): boolean;
    containsPosition(pos: number): boolean;
    /**
     * Return whether the START of the given node is visible
     *
     * For nodes that potentially span many lines (like class declarations)
     * this will check the first line.
     */
    containsStartOfNode(node: ts.Node): boolean;
    /**
     * Find the span that would contain the given position, if any
     *
     * Returns the highest span s.t. span.start <= position. Uses the fact that
     * spans are non-overlapping.
     */
    private findSpan;
}
export declare function trimCompleteSourceToVisible(source: string): string;
export interface MarkedSpan {
    start: number;
    end: number;
    visible: boolean;
}
/**
 * Whether span a is fully inside span b
 */
export declare function spanInside(a: Span, b: Span): boolean;
export declare function spanContains(a: Span, position: number): boolean;
//# sourceMappingURL=visible-spans.d.ts.map