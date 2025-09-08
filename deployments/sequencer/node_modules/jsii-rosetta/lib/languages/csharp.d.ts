import * as ts from 'typescript';
import { DefaultVisitor } from './default';
import { TargetLanguage } from './target-language';
import { ObjectLiteralStruct } from '../jsii/jsii-types';
import { OTree } from '../o-tree';
import { AstRenderer } from '../renderer';
import { ImportStatement } from '../typescript/imports';
interface CSharpLanguageContext {
    /**
     * Used to capitalize member accesses
     */
    readonly propertyOrMethod: boolean;
    /**
     * So we know how to render property signatures
     */
    readonly inStructInterface: boolean;
    /**
     * So we know how to render property signatures
     */
    readonly inRegularInterface: boolean;
    /**
     * So we know how to render property assignments
     */
    readonly inKeyValueList: boolean;
    /**
     * Whether a string literal is currently in the position of having to render as an identifier (LHS in property assignment)
     */
    readonly stringAsIdentifier: boolean;
    /**
     * Whether an identifier literal is currently in the position of having to render as a string (LHS in property assignment)
     */
    readonly identifierAsString: boolean;
    /**
     * When parsing an object literal and no type information is available, prefer parsing it as a struct to parsing it as a map
     */
    readonly preferObjectLiteralAsStruct: boolean;
    /**
     * When encountering these properties, render them as lowercase instead of uppercase
     */
    readonly privatePropertyNames: string[];
}
type CSharpRenderer = AstRenderer<CSharpLanguageContext>;
export declare class CSharpVisitor extends DefaultVisitor<CSharpLanguageContext> {
    /**
     * Translation version
     *
     * Bump this when you change something in the implementation to invalidate
     * existing cached translations.
     */
    static readonly VERSION = "1";
    readonly language = TargetLanguage.CSHARP;
    readonly defaultContext: {
        propertyOrMethod: boolean;
        inStructInterface: boolean;
        inRegularInterface: boolean;
        inKeyValueList: boolean;
        stringAsIdentifier: boolean;
        identifierAsString: boolean;
        preferObjectLiteralAsStruct: boolean;
        privatePropertyNames: never[];
    };
    /**
     * Aliases for modules
     *
     * If these are encountered in the LHS of a property access, they will be dropped.
     */
    private readonly dropPropertyAccesses;
    /**
     * Already imported modules so we don't emit duplicate imports
     */
    private readonly alreadyImportedNamespaces;
    /**
     * A map to undo import renames
     *
     * We will always reference the original name in the translation.
     *
     * Maps a local-name to a C# name.
     */
    private readonly renamedSymbols;
    mergeContext(old: CSharpLanguageContext, update: Partial<CSharpLanguageContext>): CSharpLanguageContext;
    identifier(node: ts.Identifier | ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: CSharpRenderer): OTree;
    importStatement(importStatement: ImportStatement, context: CSharpRenderer): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, renderer: CSharpRenderer): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, renderer: CSharpRenderer): OTree;
    methodDeclaration(node: ts.MethodDeclaration, renderer: CSharpRenderer): OTree;
    methodSignature(node: ts.MethodSignature, renderer: CSharpRenderer): OTree;
    functionLike(node: ts.FunctionLikeDeclaration | ts.ConstructorDeclaration | ts.MethodDeclaration, renderer: CSharpRenderer, opts?: {
        isConstructor?: boolean;
    }): OTree;
    propertyDeclaration(node: ts.PropertyDeclaration, renderer: CSharpRenderer): OTree;
    printStatement(args: ts.NodeArray<ts.Expression>, renderer: CSharpRenderer): OTree;
    superCallExpression(_node: ts.CallExpression, _renderer: CSharpRenderer): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, renderer: CSharpRenderer): OTree;
    expressionStatement(node: ts.ExpressionStatement, renderer: CSharpRenderer): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, renderer: CSharpRenderer): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, renderer: CSharpRenderer): OTree;
    propertySignature(node: ts.PropertySignature, renderer: CSharpRenderer): OTree;
    /**
     * Do some work on property accesses to translate common JavaScript-isms to language-specific idioms
     */
    regularCallExpression(node: ts.CallExpression, renderer: CSharpRenderer): OTree;
    classDeclaration(node: ts.ClassDeclaration, renderer: CSharpRenderer): OTree;
    structInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: CSharpRenderer): OTree;
    regularInterfaceDeclaration(node: ts.InterfaceDeclaration, renderer: CSharpRenderer): OTree;
    block(node: ts.Block, children: CSharpRenderer): OTree;
    unknownTypeObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: CSharpRenderer): OTree;
    knownStructObjectLiteralExpression(node: ts.ObjectLiteralExpression, structType: ObjectLiteralStruct, renderer: CSharpRenderer): OTree;
    keyValueObjectLiteralExpression(node: ts.ObjectLiteralExpression, renderer: CSharpRenderer): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, renderer: CSharpRenderer): OTree;
    propertyAssignment(node: ts.PropertyAssignment, renderer: CSharpRenderer): OTree;
    renderPropertyAssignment(key: ts.Node, value: ts.Node, renderer: CSharpRenderer): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, renderer: CSharpRenderer): OTree;
    ifStatement(node: ts.IfStatement, renderer: CSharpRenderer): OTree;
    forOfStatement(node: ts.ForOfStatement, renderer: CSharpRenderer): OTree;
    asExpression(node: ts.AsExpression, context: CSharpRenderer): OTree;
    variableDeclaration(node: ts.VariableDeclaration, renderer: CSharpRenderer): OTree;
    templateExpression(node: ts.TemplateExpression, context: CSharpRenderer): OTree;
    protected argumentList(args: readonly ts.Node[] | undefined, renderer: CSharpRenderer): OTree;
    private renderTypeNode;
    private renderType;
    private classHeritage;
}
export {};
//# sourceMappingURL=csharp.d.ts.map