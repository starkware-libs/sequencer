import * as ts from 'typescript';
import { TargetLanguage } from './languages';
import { OTree, UnknownSyntax, Span } from './o-tree';
import { SubmoduleReference, SubmoduleReferenceMap } from './submodule-reference';
import { ImportStatement } from './typescript/imports';
/**
 * Render a TypeScript AST to some other representation (encoded in OTrees)
 *
 * Dispatch the actual conversion to a specific handler which will get the
 * appropriate method called for particular AST nodes. The handler may use
 * context to modify its own operations when traversing the tree hierarchy,
 * the type of which should be expressed via the C parameter.
 */
export declare class AstRenderer<C> {
    private readonly sourceFile;
    readonly typeChecker: ts.TypeChecker;
    private readonly handler;
    private readonly options;
    readonly submoduleReferences: SubmoduleReferenceMap;
    readonly diagnostics: ts.Diagnostic[];
    readonly currentContext: C;
    constructor(sourceFile: ts.SourceFile, typeChecker: ts.TypeChecker, handler: AstHandler<C>, options?: AstRendererOptions, submoduleReferences?: SubmoduleReferenceMap);
    /**
     * Merge the new context with the current context and create a new Converter from it
     */
    updateContext(contextUpdate: Partial<C>): AstRenderer<C>;
    /**
     * Convert a single node to an OTree
     */
    convert(node: ts.Node | undefined): OTree;
    /**
     * Convert a set of nodes, filtering out hidden nodes
     */
    convertAll(nodes: readonly ts.Node[]): OTree[];
    convertWithModifier(nodes: readonly ts.Node[], makeContext: (context: this, node: ts.Node, index: number) => AstRenderer<C>): OTree[];
    /**
     * Convert a set of nodes, but update the context for the last one.
     *
     * Takes visibility into account.
     */
    convertLastDifferently(nodes: readonly ts.Node[], lastContext: C): OTree[];
    getPosition(node: ts.Node): Span;
    textOf(node: ts.Node): string;
    textAt(pos: number, end: number): string;
    /**
     * Infer type of expression by the argument it is assigned to
     *
     * If the type of the expression can include undefined (if the value is
     * optional), `undefined` will be removed from the union.
     *
     * (Will return undefined for object literals not unified with a declared type)
     *
     * @deprecated Use `inferredTypeOfExpression` instead
     */
    inferredTypeOfExpression(node: ts.Expression): ts.Type | undefined;
    /**
     * Type of expression from the text of the expression
     *
     * (Will return a map type for object literals)
     *
     * @deprecated Use `typeOfExpression` directly
     */
    typeOfExpression(node: ts.Expression): ts.Type;
    typeOfType(node: ts.TypeNode): ts.Type;
    typeToString(type: ts.Type): string;
    report(node: ts.Node, messageText: string, category?: ts.DiagnosticCategory): void;
    reportUnsupported(node: ts.Node, language: TargetLanguage | undefined): void;
    /**
     * Whether there is non-whitespace on the same line before the given position
     */
    codeOnLineBefore(pos: number): boolean;
    /**
     * Return a newline if the given node is preceded by at least one newline
     *
     * Used to mirror newline use between matchin brackets (such as { ... } and [ ... ]).
     */
    mirrorNewlineBefore(viz?: ts.Node, suffix?: string, otherwise?: string): string;
    /**
     * Dispatch node to handler
     */
    private dispatch;
    /**
     * Attach any leading whitespace and comments to the given output tree
     *
     * Regardless of whether it's declared to be able to accept such or not.
     */
    private attachLeadingTrivia;
}
/**
 * Interface for AST handlers
 *
 * C is the type of hierarchical context the handler uses. Context
 * needs 2 operations: a constructor for a default context, and a
 * merge operation to combine 2 contexts to yield a new one.
 *
 * Otherwise, the handler should return an OTree for every type
 * of AST node.
 */
export interface AstHandler<C> {
    readonly language: TargetLanguage;
    readonly defaultContext: C;
    readonly indentChar?: ' ' | '\t';
    mergeContext(old: C, update: Partial<C>): C;
    sourceFile(node: ts.SourceFile, context: AstRenderer<C>): OTree;
    commentRange(node: CommentSyntax, context: AstRenderer<C>): OTree;
    importStatement(node: ImportStatement, context: AstRenderer<C>): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, children: AstRenderer<C>): OTree;
    numericLiteral(node: ts.NumericLiteral, children: AstRenderer<C>): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, children: AstRenderer<C>): OTree;
    identifier(node: ts.Identifier, children: AstRenderer<C>): OTree;
    block(node: ts.Block, children: AstRenderer<C>): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, children: AstRenderer<C>): OTree;
    returnStatement(node: ts.ReturnStatement, context: AstRenderer<C>): OTree;
    binaryExpression(node: ts.BinaryExpression, context: AstRenderer<C>): OTree;
    ifStatement(node: ts.IfStatement, context: AstRenderer<C>): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, context: AstRenderer<C>, submoduleReference: SubmoduleReference | undefined): OTree;
    awaitExpression(node: ts.AwaitExpression, context: AstRenderer<C>): OTree;
    callExpression(node: ts.CallExpression, context: AstRenderer<C>): OTree;
    expressionStatement(node: ts.ExpressionStatement, context: AstRenderer<C>): OTree;
    token<A extends ts.SyntaxKind>(node: ts.Token<A>, context: AstRenderer<C>): OTree;
    objectLiteralExpression(node: ts.ObjectLiteralExpression, context: AstRenderer<C>): OTree;
    newExpression(node: ts.NewExpression, context: AstRenderer<C>): OTree;
    propertyAssignment(node: ts.PropertyAssignment, context: AstRenderer<C>): OTree;
    variableStatement(node: ts.VariableStatement, context: AstRenderer<C>): OTree;
    variableDeclarationList(node: ts.VariableDeclarationList, context: AstRenderer<C>): OTree;
    variableDeclaration(node: ts.VariableDeclaration, context: AstRenderer<C>): OTree;
    jsDoc(node: ts.JSDoc, context: AstRenderer<C>): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, context: AstRenderer<C>): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, context: AstRenderer<C>): OTree;
    forOfStatement(node: ts.ForOfStatement, context: AstRenderer<C>): OTree;
    classDeclaration(node: ts.ClassDeclaration, context: AstRenderer<C>): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, context: AstRenderer<C>): OTree;
    propertyDeclaration(node: ts.PropertyDeclaration, context: AstRenderer<C>): OTree;
    computedPropertyName(node: ts.Expression, context: AstRenderer<C>): OTree;
    methodDeclaration(node: ts.MethodDeclaration, context: AstRenderer<C>): OTree;
    interfaceDeclaration(node: ts.InterfaceDeclaration, context: AstRenderer<C>): OTree;
    propertySignature(node: ts.PropertySignature, context: AstRenderer<C>): OTree;
    methodSignature(node: ts.MethodSignature, context: AstRenderer<C>): OTree;
    asExpression(node: ts.AsExpression, context: AstRenderer<C>): OTree;
    prefixUnaryExpression(node: ts.PrefixUnaryExpression, context: AstRenderer<C>): OTree;
    spreadElement(node: ts.SpreadElement, context: AstRenderer<C>): OTree;
    spreadAssignment(node: ts.SpreadAssignment, context: AstRenderer<C>): OTree;
    templateExpression(node: ts.TemplateExpression, context: AstRenderer<C>): OTree;
    nonNullExpression(node: ts.NonNullExpression, context: AstRenderer<C>): OTree;
    parenthesizedExpression(node: ts.ParenthesizedExpression, context: AstRenderer<C>): OTree;
    maskingVoidExpression(node: ts.VoidExpression, context: AstRenderer<C>): OTree;
    elementAccessExpression(node: ts.ElementAccessExpression, context: AstRenderer<C>): OTree;
    ellipsis(node: ts.SpreadElement | ts.SpreadAssignment, context: AstRenderer<C>): OTree;
}
export declare function nimpl<C>(node: ts.Node, context: AstRenderer<C>, options?: {
    additionalInfo?: string;
}): UnknownSyntax;
export interface AstRendererOptions {
    /**
     * If enabled, don't translate the text of unknown nodes
     *
     * @default true
     */
    bestEffort?: boolean;
}
/**
 * Our own representation of comments
 *
 * (So we can synthesize 'em
 */
export interface CommentSyntax {
    pos: number;
    text: string;
    hasTrailingNewLine?: boolean;
    kind: ts.CommentKind;
    /**
     * Whether it's at the end of a code line (so we can render a separating space)
     */
    isTrailing?: boolean;
}
//# sourceMappingURL=renderer.d.ts.map