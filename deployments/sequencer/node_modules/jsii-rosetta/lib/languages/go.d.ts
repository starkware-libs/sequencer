import * as ts from 'typescript';
import { DefaultVisitor } from './default';
import { TargetLanguage } from './target-language';
import { ObjectLiteralStruct } from '../jsii/jsii-types';
import { OTree } from '../o-tree';
import { AstRenderer } from '../renderer';
import { SubmoduleReference } from '../submodule-reference';
import { ImportStatement } from '../typescript/imports';
interface GoLanguageContext {
    /**
     * Free floating symbols are made importable across packages by naming with a capital in Go.
     */
    isExported: boolean;
    /**
     * Whether type should be converted a pointer type
     */
    isPtr: boolean;
    /**
     * Whether this is the R-Value in an assignment expression to a pointer value.
     */
    isPtrAssignmentRValue: boolean;
    /**
     * Whether the current element is a parameter delcaration name.
     */
    isParameterName: boolean;
    /**
     * Whether context is within a struct declaration
     */
    isStruct: boolean;
    /**
     * Whether the context is within an interface delcaration.
     */
    isInterface: boolean;
    /**
     * Whether properties are being intialized within a `map` type
     */
    inMapLiteral: boolean;
    /**
     * Wheter to wrap a literal in a pointer constructor ie: jsii.String.
     */
    wrapPtr: boolean;
}
type GoRenderer = AstRenderer<GoLanguageContext>;
export declare class GoVisitor extends DefaultVisitor<GoLanguageContext> {
    /**
     * Translation version
     *
     * Bump this when you change something in the implementation to invalidate
     * existing cached translations.
     */
    static readonly VERSION = "1";
    readonly indentChar = "\t";
    readonly language = TargetLanguage.GO;
    private readonly idMap;
    readonly defaultContext: GoLanguageContext;
    protected argumentList(args: readonly ts.Node[] | undefined, renderer: GoRenderer): OTree;
    block(node: ts.Block, renderer: GoRenderer): OTree;
    expressionStatement(node: ts.ExpressionStatement, renderer: GoRenderer): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, renderer: GoRenderer): OTree;
    identifier(node: ts.Identifier | ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: GoRenderer): OTree;
    newExpression(node: ts.NewExpression, renderer: GoRenderer): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, renderer: AstRenderer<GoLanguageContext>): OTree;
    objectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: GoRenderer): OTree;
    propertyAssignment(node: ts.PropertyAssignment, renderer: GoRenderer): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, renderer: AstRenderer<GoLanguageContext>): OTree;
    templateExpression(node: ts.TemplateExpression, renderer: AstRenderer<GoLanguageContext>): OTree;
    token<A extends ts.SyntaxKind>(node: ts.Token<A>, renderer: GoRenderer): OTree;
    unknownTypeObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: GoRenderer): OTree;
    keyValueObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: GoRenderer): OTree;
    knownStructObjectLiteralExpression(node: ts.ObjectLiteralExpression, structType: ObjectLiteralStruct, renderer: GoRenderer): OTree;
    asExpression(node: ts.AsExpression, renderer: AstRenderer<GoLanguageContext>): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, renderer: GoRenderer): OTree;
    printStatement(args: ts.NodeArray<ts.Expression>, renderer: GoRenderer): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, renderer: GoRenderer, submoduleReference?: SubmoduleReference): OTree;
    methodSignature(node: ts.MethodSignature, renderer: AstRenderer<GoLanguageContext>): OTree;
    propertyDeclaration(node: ts.PropertyDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    propertySignature(node: ts.PropertySignature, renderer: GoRenderer): OTree;
    regularCallExpression(node: ts.CallExpression, renderer: GoRenderer): OTree;
    returnStatement(node: ts.ReturnStatement, renderer: AstRenderer<GoLanguageContext>): OTree;
    binaryExpression(node: ts.BinaryExpression, renderer: AstRenderer<GoLanguageContext>): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: GoRenderer): OTree;
    numericLiteral(node: ts.NumericLiteral, renderer: GoRenderer): OTree;
    classDeclaration(node: ts.ClassDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    structInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: GoRenderer): OTree;
    regularInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    superCallExpression(node: ts.CallExpression, renderer: AstRenderer<GoLanguageContext>): OTree;
    methodDeclaration(node: ts.MethodDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    ifStatement(node: ts.IfStatement, renderer: AstRenderer<GoLanguageContext>): OTree;
    forOfStatement(node: ts.ForOfStatement, renderer: AstRenderer<GoLanguageContext>): OTree;
    importStatement(node: ImportStatement, renderer: AstRenderer<GoLanguageContext>): OTree;
    variableDeclaration(node: ts.VariableDeclaration, renderer: AstRenderer<GoLanguageContext>): OTree;
    private defaultArgValues;
    mergeContext(old: GoLanguageContext, update: Partial<GoLanguageContext>): GoLanguageContext;
    private renderTypeNode;
    private renderType;
    /**
     * Guess an item's go name based on it's TS name and context
     */
    private goName;
}
export {};
//# sourceMappingURL=go.d.ts.map