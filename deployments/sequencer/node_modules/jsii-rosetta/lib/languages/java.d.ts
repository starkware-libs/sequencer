import * as ts from 'typescript';
import { DefaultVisitor } from './default';
import { ObjectLiteralStruct } from '../jsii/jsii-types';
import { TargetLanguage } from '../languages/target-language';
import { OTree } from '../o-tree';
import { AstRenderer } from '../renderer';
import { SubmoduleReference } from '../submodule-reference';
import { ImportStatement } from '../typescript/imports';
interface JavaContext {
    /**
     * Whether to ignore the left-hand part of a property access expression.
     *
     * Used to strip out TypeScript namespace prefixes from 'extends' and 'new' clauses,
     * EVEN if the source doesn't compile.
     *
     * @default false
     */
    readonly discardPropertyAccess?: boolean;
    /**
     * Whether a property access ('sth.b') should be substituted by a getter ('sth.getB()').
     * Not true for 'new' expressions and calls to methods on objects.
     *
     * @default true
     */
    readonly convertPropertyToGetter?: boolean;
    /**
     * Set when we are in the middle translating a type (= class, interface or enum) declaration.
     */
    readonly insideTypeDeclaration?: InsideTypeDeclaration;
    /**
     * True if we are in the middle of a `new` expression that has an object literal as its last argument -
     * in that case, we render a ClassName.Builder.create(...).prop(...).build() expression instead.
     *
     * @default false
     */
    readonly inNewExprWithObjectLiteralAsLastArg?: boolean;
    /**
     * True when, from the context,
     * we are supposed to render a JavaScript object literal as a Map in Java.
     *
     * @default false
     */
    readonly inKeyValueList?: boolean;
    /**
     * Used when rendering a JavaScript object literal that is _not_ for a struct -
     * we render that as a Map in Java.
     *
     * @default false
     */
    readonly identifierAsString?: boolean;
    /**
     * Used when rendering a JavaScript object literal that is for a struct -
     * maps to a Builder in Java.
     *
     * @default false
     */
    readonly stringLiteralAsIdentifier?: boolean;
    /**
     * Used to denote that a type is being rendered in a position where a generic
     * type parameter is expected, so only reference types are valid (not
     * primitives).
     *
     * @default false
     */
    readonly requiresReferenceType?: boolean;
}
/**
 * Values saved when we are translating a type declaration.
 */
interface InsideTypeDeclaration {
    /**
     * The name of the type.
     * Needed to correctly generate the constructor.
     */
    readonly typeName: ts.Node | undefined;
    /**
     * Is this an interface (true) or a class (unset/false)
     */
    readonly isInterface?: boolean;
}
type JavaRenderer = AstRenderer<JavaContext>;
export declare class JavaVisitor extends DefaultVisitor<JavaContext> {
    /**
     * Translation version
     *
     * Bump this when you change something in the implementation to invalidate
     * existing cached translations.
     */
    static readonly VERSION = "1";
    /**
     * Aliases for modules
     *
     * If these are encountered in the LHS of a property access, they will be dropped.
     */
    private readonly dropPropertyAccesses;
    readonly language = TargetLanguage.JAVA;
    readonly defaultContext: {};
    mergeContext(old: JavaContext, update: Partial<JavaContext>): JavaContext;
    importStatement(importStatement: ImportStatement): OTree;
    classDeclaration(node: ts.ClassDeclaration, renderer: JavaRenderer): OTree;
    structInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: JavaRenderer): OTree;
    regularInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: JavaRenderer): OTree;
    propertySignature(node: ts.PropertySignature, renderer: JavaRenderer): OTree;
    propertyDeclaration(node: ts.PropertyDeclaration, renderer: JavaRenderer): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, renderer: JavaRenderer): OTree;
    methodDeclaration(node: ts.MethodDeclaration, renderer: JavaRenderer): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, renderer: JavaRenderer): OTree;
    methodSignature(node: ts.MethodSignature, renderer: JavaRenderer): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, renderer: JavaRenderer): OTree;
    block(node: ts.Block, renderer: JavaRenderer): OTree;
    variableDeclaration(node: ts.VariableDeclaration, renderer: JavaRenderer): OTree;
    expressionStatement(node: ts.ExpressionStatement, renderer: JavaRenderer): OTree;
    ifStatement(node: ts.IfStatement, renderer: JavaRenderer): OTree;
    forOfStatement(node: ts.ForOfStatement, renderer: JavaRenderer): OTree;
    printStatement(args: ts.NodeArray<ts.Expression>, renderer: JavaRenderer): OTree;
    templateExpression(node: ts.TemplateExpression, renderer: JavaRenderer): OTree;
    asExpression(node: ts.AsExpression, renderer: JavaRenderer): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, renderer: JavaRenderer): OTree;
    regularCallExpression(node: ts.CallExpression, renderer: JavaRenderer): OTree;
    newExpression(node: ts.NewExpression, renderer: JavaRenderer): OTree;
    unknownTypeObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: JavaRenderer): OTree;
    keyValueObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: JavaRenderer): OTree;
    knownStructObjectLiteralExpression(node: ts.ObjectLiteralExpression, structType: ObjectLiteralStruct, renderer: JavaRenderer): OTree;
    propertyAssignment(node: ts.PropertyAssignment, renderer: JavaRenderer): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, renderer: JavaRenderer): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, renderer: JavaRenderer, submoduleRef: SubmoduleReference | undefined): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: JavaRenderer): OTree;
    identifier(node: ts.Identifier | ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: JavaRenderer): OTree;
    private renderObjectLiteralAsBuilder;
    private singlePropertyInJavaScriptObjectLiteralToJavaMap;
    private singlePropertyInJavaScriptObjectLiteralToFluentSetters;
    private renderClassDeclaration;
    private typeHeritage;
    private extractSuperTypes;
    private renderTypeNode;
    private renderType;
    private renderProcedure;
    private renderOverload;
    private renderBlock;
}
export {};
//# sourceMappingURL=java.d.ts.map