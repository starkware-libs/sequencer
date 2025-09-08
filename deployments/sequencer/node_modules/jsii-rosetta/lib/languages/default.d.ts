import * as ts from 'typescript';
import type { TargetLanguage } from './target-language';
import { ObjectLiteralStruct } from '../jsii/jsii-types';
import { OTree } from '../o-tree';
import { AstRenderer, AstHandler, CommentSyntax } from '../renderer';
import { SubmoduleReference } from '../submodule-reference';
import { ImportStatement } from '../typescript/imports';
/**
 * A basic visitor that applies for most curly-braces-based languages
 */
export declare abstract class DefaultVisitor<C> implements AstHandler<C> {
    abstract readonly defaultContext: C;
    abstract readonly language: TargetLanguage;
    protected statementTerminator: string;
    abstract mergeContext(old: C, update: C): C;
    commentRange(comment: CommentSyntax, _context: AstRenderer<C>): OTree;
    sourceFile(node: ts.SourceFile, context: AstRenderer<C>): OTree;
    jsDoc(_node: ts.JSDoc, _context: AstRenderer<C>): OTree;
    importStatement(node: ImportStatement, context: AstRenderer<C>): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, children: AstRenderer<C>): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, _renderer: AstRenderer<C>): OTree;
    numericLiteral(node: ts.NumericLiteral, _children: AstRenderer<C>): OTree;
    identifier(node: ts.Identifier, _children: AstRenderer<C>): OTree;
    block(node: ts.Block, children: AstRenderer<C>): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, children: AstRenderer<C>): OTree;
    returnStatement(node: ts.ReturnStatement, children: AstRenderer<C>): OTree;
    binaryExpression(node: ts.BinaryExpression, context: AstRenderer<C>): OTree;
    prefixUnaryExpression(node: ts.PrefixUnaryExpression, context: AstRenderer<C>): OTree;
    translateUnaryOperator(operator: ts.PrefixUnaryOperator): string;
    translateBinaryOperator(operator: string): string;
    ifStatement(node: ts.IfStatement, context: AstRenderer<C>): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, context: AstRenderer<C>, _submoduleReference: SubmoduleReference | undefined): OTree;
    /**
     * Do some work on property accesses to translate common JavaScript-isms to language-specific idioms
     */
    callExpression(node: ts.CallExpression, context: AstRenderer<C>): OTree;
    awaitExpression(node: ts.AwaitExpression, context: AstRenderer<C>): OTree;
    regularCallExpression(node: ts.CallExpression, context: AstRenderer<C>): OTree;
    superCallExpression(node: ts.CallExpression, context: AstRenderer<C>): OTree;
    printStatement(args: ts.NodeArray<ts.Expression>, context: AstRenderer<C>): OTree;
    expressionStatement(node: ts.ExpressionStatement, context: AstRenderer<C>): OTree;
    token<A extends ts.SyntaxKind>(node: ts.Token<A>, context: AstRenderer<C>): OTree;
    /**
     * An object literal can render as one of three things:
     *
     * - Don't know the type (render as an unknown struct)
     * - Know the type:
     *     - It's a struct (render as known struct)
     *     - It's not a struct (render as key-value map)
     */
    objectLiteralExpression(node: ts.ObjectLiteralExpression, context: AstRenderer<C>): OTree;
    unknownTypeObjectLiteralExpression(node: ts.ObjectLiteralExpression, context: AstRenderer<C>): OTree;
    knownStructObjectLiteralExpression(node: ts.ObjectLiteralExpression, _structType: ObjectLiteralStruct, context: AstRenderer<C>): OTree;
    keyValueObjectLiteralExpression(node: ts.ObjectLiteralExpression, context: AstRenderer<C>): OTree;
    newExpression(node: ts.NewExpression, context: AstRenderer<C>): OTree;
    propertyAssignment(node: ts.PropertyAssignment, context: AstRenderer<C>): OTree;
    variableStatement(node: ts.VariableStatement, context: AstRenderer<C>): OTree;
    variableDeclarationList(node: ts.VariableDeclarationList, context: AstRenderer<C>): OTree;
    variableDeclaration(node: ts.VariableDeclaration, context: AstRenderer<C>): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, context: AstRenderer<C>): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, context: AstRenderer<C>): OTree;
    forOfStatement(node: ts.ForOfStatement, context: AstRenderer<C>): OTree;
    classDeclaration(node: ts.ClassDeclaration, context: AstRenderer<C>): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, context: AstRenderer<C>): OTree;
    propertyDeclaration(node: ts.PropertyDeclaration, context: AstRenderer<C>): OTree;
    computedPropertyName(node: ts.Expression, context: AstRenderer<C>): OTree;
    methodDeclaration(node: ts.MethodDeclaration, context: AstRenderer<C>): OTree;
    interfaceDeclaration(node: ts.InterfaceDeclaration, context: AstRenderer<C>): OTree;
    structInterfaceDeclaration(node: ts.InterfaceDeclaration, context: AstRenderer<C>): OTree;
    regularInterfaceDeclaration(node: ts.InterfaceDeclaration, context: AstRenderer<C>): OTree;
    propertySignature(node: ts.PropertySignature, context: AstRenderer<C>): OTree;
    methodSignature(node: ts.MethodSignature, context: AstRenderer<C>): OTree;
    asExpression(node: ts.AsExpression, context: AstRenderer<C>): OTree;
    spreadElement(node: ts.SpreadElement, context: AstRenderer<C>): OTree;
    spreadAssignment(node: ts.SpreadAssignment, context: AstRenderer<C>): OTree;
    ellipsis(_node: ts.SpreadElement | ts.SpreadAssignment, _context: AstRenderer<C>): OTree;
    templateExpression(node: ts.TemplateExpression, context: AstRenderer<C>): OTree;
    elementAccessExpression(node: ts.ElementAccessExpression, context: AstRenderer<C>): OTree;
    nonNullExpression(node: ts.NonNullExpression, context: AstRenderer<C>): OTree;
    parenthesizedExpression(node: ts.ParenthesizedExpression, context: AstRenderer<C>): OTree;
    maskingVoidExpression(node: ts.VoidExpression, context: AstRenderer<C>): OTree;
    protected argumentList(args: readonly ts.Node[] | undefined, context: AstRenderer<C>): OTree;
    private notImplemented;
}
//# sourceMappingURL=default.d.ts.map