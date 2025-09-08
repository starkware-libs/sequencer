import * as ts from 'typescript';
import { AstRenderer } from '../renderer';
export declare function stripCommentMarkers(comment: string, multiline: boolean): string;
export declare function stringFromLiteral(expr: ts.Expression): string;
/**
 * All types of nodes that can be captured using `nodeOfType`, and the type of Node they map to
 */
export type CapturableNodes = {
    [ts.SyntaxKind.ImportDeclaration]: ts.ImportDeclaration;
    [ts.SyntaxKind.VariableDeclaration]: ts.VariableDeclaration;
    [ts.SyntaxKind.ExternalModuleReference]: ts.ExternalModuleReference;
    [ts.SyntaxKind.NamespaceImport]: ts.NamespaceImport;
    [ts.SyntaxKind.NamedImports]: ts.NamedImports;
    [ts.SyntaxKind.ImportSpecifier]: ts.ImportSpecifier;
    [ts.SyntaxKind.StringLiteral]: ts.StringLiteral;
};
export type AstMatcher<A> = (nodes?: ts.Node[]) => A | undefined;
/**
 * Return AST children of the given node
 *
 * Difference with node.getChildren():
 *
 * - node.getChildren() must take a SourceFile (will fail if it doesn't get it)
 *   and returns a mix of abstract and concrete syntax nodes.
 * - This function function will ONLY return abstract syntax nodes.
 */
export declare function nodeChildren(node: ts.Node): ts.Node[];
/**
 * Match a single node of a given type
 *
 * Capture name is first so that the IDE can detect eagerly that we're falling into
 * that overload and properly autocomplete the recognized node types from CapturableNodes.
 *
 * Looks like SyntaxList nodes appear in the printed AST, but they don't actually appear
 */
export declare function nodeOfType<A>(syntaxKind: ts.SyntaxKind, children?: AstMatcher<A>): AstMatcher<A>;
export declare function nodeOfType<S extends keyof CapturableNodes, N extends string, A>(capture: N, capturableNodeType: S, children?: AstMatcher<A>): AstMatcher<Omit<A, N> & {
    [key in N]: CapturableNodes[S];
}>;
export declare function anyNode(): AstMatcher<Record<string, unknown>>;
export declare function anyNode<A>(children: AstMatcher<A>): AstMatcher<A>;
export declare function allOfType<S extends keyof CapturableNodes, N extends string, A>(s: S, name: N, children?: AstMatcher<A>): AstMatcher<{
    [key in N]: Array<CapturableNodes[S]>;
}>;
export declare const DONE: AstMatcher<Record<string, unknown>>;
/**
 * Run a matcher against a node and return (or invoke a callback with) the accumulated bindings
 */
export declare function matchAst<A>(node: ts.Node, matcher: AstMatcher<A>): A | undefined;
export declare function matchAst<A>(node: ts.Node, matcher: AstMatcher<A>, cb: (bindings: A) => void): boolean;
/**
 * Count the newlines in a given piece of string that aren't in comment blocks
 */
export declare function countNakedNewlines(str: string): number;
export declare function repeatNewlines(str: string): string;
/**
 * Extract single-line and multi-line comments from the given string
 *
 * Rewritten because I can't get ts.getLeadingComments and ts.getTrailingComments to do what I want.
 */
export declare function extractComments(text: string, start: number): ts.CommentRange[];
export declare function commentRangeFromTextRange(rng: TextRange): ts.CommentRange;
interface TextRange {
    pos: number;
    end: number;
    type: 'linecomment' | 'blockcomment' | 'other' | 'directive';
    hasTrailingNewLine: boolean;
}
/**
 * Extract spans of comments and non-comments out of the string
 *
 * Stop at 'end' when given, or the first non-whitespace character in a
 * non-comment if not given.
 */
export declare function scanText(text: string, start: number, end?: number): TextRange[];
export declare function extractMaskingVoidExpression(node: ts.Node): ts.VoidExpression | undefined;
export declare function extractShowingVoidExpression(node: ts.Node): ts.VoidExpression | undefined;
/**
 * Return the string argument to a void expression if it exists
 */
export declare function voidExpressionString(node: ts.VoidExpression): string | undefined;
/**
 * We use void directives as pragmas. Extract the void directives here
 */
export declare function extractVoidExpression(node: ts.Node): ts.VoidExpression | undefined;
export declare function quoteStringLiteral(x: string): string;
export declare function visibility(x: ts.AccessorDeclaration | ts.FunctionLikeDeclaration | ts.GetAccessorDeclaration | ts.PropertyDeclaration | ts.PropertySignature | ts.SetAccessorDeclaration): "private" | "protected" | "public";
export declare const isReadOnly: (x: ts.GetAccessorDeclaration | ts.SetAccessorDeclaration | ts.ArrowFunction | ts.ConstructorDeclaration | ts.FunctionDeclaration | ts.FunctionExpression | ts.MethodDeclaration | ts.PropertyDeclaration | ts.PropertySignature) => boolean;
export declare const isExported: (x: ts.Declaration) => boolean;
export declare const isPrivate: (x: ts.Declaration) => boolean;
export declare const isProtected: (x: ts.Declaration) => boolean;
export declare function isPublic(x: ts.Declaration): boolean;
export declare const isStatic: (x: ts.Declaration) => boolean;
/**
 * Return the super() call from a method body if found
 */
export declare function findSuperCall(node: ts.Block | ts.Expression | undefined, renderer: AstRenderer<any>): ts.SuperCall | undefined;
/**
 * Return the names of all private property declarations
 */
export declare function privatePropertyNames(members: readonly ts.ClassElement[], renderer: AstRenderer<any>): string[];
export declare function findEnclosingClassDeclaration(node: ts.Node): ts.ClassDeclaration | undefined;
export {};
//# sourceMappingURL=ast-utils.d.ts.map