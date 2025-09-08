import { Spans } from './typescript/visible-spans';
export interface OTreeOptions {
    /**
     * Adjust indentation with the given number
     *
     * Indentation affects children.
     *
     * @default 0
     */
    indent?: number;
    /**
     * Separate children with the given string
     *
     * @default ''
     */
    separator?: string;
    /**
     * Whether trailing separators should be output. This imples children will be
     * writen each on a new line.
     *
     * @default false
     */
    trailingSeparator?: boolean;
    /**
     * Suffix the token after outdenting
     *
     * @default ''
     */
    suffix?: string;
    /**
     * Whether this part of the generated syntax is okay to insert newlines and comments
     *
     * @default false
     */
    canBreakLine?: boolean;
    /**
     * If set, a unique key which will cause only one node with the given key to be rendered.
     *
     * The outermost key is the one that will be rendered.
     *
     * Used to make it easier to keep the state necessary to render comments
     * only once in the output tree, rather than keep the state in the
     * language rendered.
     *
     * @default No conditional rendering
     */
    renderOnce?: string;
}
/**
 * "Output" Tree
 *
 * Tree-like structure that holds sequences of trees and strings, which
 * can be rendered to an output sink.
 */
export declare class OTree implements OTree {
    private readonly options;
    static simplify(xs: Array<OTree | string | undefined>): Array<OTree | string>;
    readonly attachComment: boolean;
    private readonly prefix;
    private readonly children;
    private span?;
    constructor(prefix: Array<OTree | string | undefined>, children?: Array<OTree | string | undefined>, options?: OTreeOptions);
    /**
     * Set the span in the source file this tree node relates to
     */
    setSpan(start: number, end: number): void;
    write(sink: OTreeSink): void;
    get isEmpty(): boolean;
    toString(): string;
}
export declare const NO_SYNTAX: OTree;
export declare class UnknownSyntax extends OTree {
}
export interface SinkMark {
    readonly wroteNonWhitespaceSinceMark: boolean;
}
export interface OTreeSinkOptions {
    /**
     * @default ' '
     */
    indentChar?: ' ' | '\t';
    visibleSpans?: Spans;
}
/**
 * Output sink for OTree objects
 *
 * Maintains state about what has been rendered supports suppressing code
 * fragments based on their tagged source location.
 *
 * Basically: manages the state that was too hard to manage in the
 * tree :).
 */
export declare class OTreeSink {
    private readonly options;
    private readonly indentChar;
    private readonly indentLevels;
    private readonly fragments;
    private readonly singletonsRendered;
    private pendingIndentChange;
    private rendering;
    constructor(options?: OTreeSinkOptions);
    tagOnce(key: string | undefined): boolean;
    /**
     * Get a mark for the current sink output location
     *
     * Marks can be used to query about things that have been written to output.
     */
    mark(): SinkMark;
    write(text: string | OTree): void;
    /**
     * Ensures the following tokens will be output on a new line (emits a new line
     * and indent unless immediately preceded or followed by a newline, ignoring
     * surrounding white space).
     */
    ensureNewLine(): void;
    renderingForSpan(span?: Span): boolean;
    requestIndentChange(x: number): () => void;
    toString(): string;
    private append;
    private applyPendingIndentChange;
    private get currentIndent();
}
export declare function renderTree(tree: OTree, options?: OTreeSinkOptions): string;
export interface Span {
    readonly start: number;
    readonly end: number;
}
//# sourceMappingURL=o-tree.d.ts.map