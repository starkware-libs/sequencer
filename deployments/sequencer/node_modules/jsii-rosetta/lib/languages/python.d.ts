import * as ts from 'typescript';
import { DefaultVisitor } from './default';
import { ObjectLiteralStruct } from '../jsii/jsii-types';
import { TargetLanguage } from '../languages/target-language';
import { OTree } from '../o-tree';
import { AstRenderer, CommentSyntax } from '../renderer';
import { SubmoduleReference } from '../submodule-reference';
import { ImportStatement } from '../typescript/imports';
interface StructVar {
    variableName: string;
    type: ts.Type | undefined;
}
type ReturnFromTree<A> = {
    value?: A;
};
interface PythonLanguageContext {
    /**
     * Whether we're currently rendering a parameter in tail position
     *
     * If so, and the parameter is of type struct, explode it to keyword args
     * and return its information in `returnExplodedParameter`.
     */
    readonly tailPositionParameter?: boolean;
    /**
     * Used to return details about any exploded parameter
     */
    readonly returnExplodedParameter?: ReturnFromTree<StructVar>;
    /**
     * Whether we're currently rendering a value/expression in tail position
     *
     * If so, and the expression seems to be of a struct type, explode it
     * to keyword args.
     */
    readonly tailPositionArgument?: boolean;
    /**
     * Whether object literal members should render themselves as dict
     * members or keyword args
     */
    readonly renderObjectLiteralAsKeywords?: boolean;
    /**
     * In a code block, if any parameter is exploded, information about the parameter here
     */
    readonly explodedParameter?: StructVar;
    /**
     * Whether we're rendering a method or property inside a class
     */
    readonly inClass?: boolean;
    /**
     * Whether the current property assignment is in the context of a map value.
     * In this case, the keys should be strings (quoted where needed), and should
     * not get mangled or case-converted.
     */
    readonly inMap?: boolean;
    /**
     * If we're in a method, what is it's name
     *
     * (Used to render super() call.);
     */
    readonly currentMethodName?: string;
    /**
     * If we're rendering a variadic argument value
     */
    readonly variadicArgument?: boolean;
}
type PythonVisitorContext = AstRenderer<PythonLanguageContext>;
export interface PythonVisitorOptions {
    disclaimer?: string;
}
export declare class PythonVisitor extends DefaultVisitor<PythonLanguageContext> {
    private readonly options;
    /**
     * Translation version
     *
     * Bump this when you change something in the implementation to invalidate
     * existing cached translations.
     */
    static readonly VERSION = "2";
    readonly language = TargetLanguage.PYTHON;
    readonly defaultContext: {};
    /**
     * Keep track of module imports we've seen, so that if we need to render a type we can pick from these modules
     */
    private readonly imports;
    /**
     * Synthetic imports that need to be added as a final step
     */
    private readonly syntheticImportsToAdd;
    protected statementTerminator: string;
    constructor(options?: PythonVisitorOptions);
    mergeContext(old: PythonLanguageContext, update: Partial<PythonLanguageContext>): PythonLanguageContext & Partial<PythonLanguageContext>;
    commentRange(comment: CommentSyntax, _context: PythonVisitorContext): OTree;
    sourceFile(node: ts.SourceFile, context: PythonVisitorContext): OTree;
    importStatement(node: ImportStatement, context: PythonVisitorContext): OTree;
    token<A extends ts.SyntaxKind>(node: ts.Token<A>, context: PythonVisitorContext): OTree;
    identifier(node: ts.Identifier, context: PythonVisitorContext): OTree;
    functionDeclaration(node: ts.FunctionDeclaration, context: PythonVisitorContext): OTree;
    constructorDeclaration(node: ts.ConstructorDeclaration, context: PythonVisitorContext): OTree;
    methodDeclaration(node: ts.MethodDeclaration, context: PythonVisitorContext): OTree;
    expressionStatement(node: ts.ExpressionStatement, context: PythonVisitorContext): OTree;
    functionLike(node: ts.FunctionLikeDeclarationBase, context: PythonVisitorContext, opts?: {
        isConstructor?: boolean;
    }): OTree;
    block(node: ts.Block, context: PythonVisitorContext): OTree;
    regularCallExpression(node: ts.CallExpression, context: PythonVisitorContext): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, context: PythonVisitorContext, submoduleReference: SubmoduleReference | undefined): OTree;
    parameterDeclaration(node: ts.ParameterDeclaration, context: PythonVisitorContext): OTree;
    ifStatement(node: ts.IfStatement, context: PythonVisitorContext): OTree;
    unknownTypeObjectLiteralExpression(node: ts.ObjectLiteralExpression, context: PythonVisitorContext): OTree;
    knownStructObjectLiteralExpression(node: ts.ObjectLiteralExpression, structType: ObjectLiteralStruct, context: PythonVisitorContext): OTree;
    keyValueObjectLiteralExpression(node: ts.ObjectLiteralExpression, context: PythonVisitorContext): OTree;
    translateUnaryOperator(operator: ts.PrefixUnaryOperator): string;
    renderObjectLiteralExpression(prefix: string, suffix: string, renderObjectLiteralAsKeywords: boolean, node: ts.ObjectLiteralExpression, context: PythonVisitorContext): OTree;
    arrayLiteralExpression(node: ts.ArrayLiteralExpression, context: PythonVisitorContext): OTree;
    propertyAssignment(node: ts.PropertyAssignment, context: PythonVisitorContext): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, context: PythonVisitorContext): OTree;
    newExpression(node: ts.NewExpression, context: PythonVisitorContext): OTree;
    variableDeclaration(node: ts.VariableDeclaration, context: PythonVisitorContext): OTree;
    thisKeyword(): OTree;
    forOfStatement(node: ts.ForOfStatement, context: PythonVisitorContext): OTree;
    classDeclaration(node: ts.ClassDeclaration, context: PythonVisitorContext): OTree;
    printStatement(args: ts.NodeArray<ts.Expression>, context: PythonVisitorContext): OTree;
    propertyDeclaration(_node: ts.PropertyDeclaration, _context: PythonVisitorContext): OTree;
    /**
     * We have to do something special here
     *
     * Best-effort, we remember the fields of struct interfaces and keep track of
     * them. Fortunately we can determine from the name whether what to do.
     */
    interfaceDeclaration(_node: ts.InterfaceDeclaration, _context: PythonVisitorContext): OTree;
    propertySignature(_node: ts.PropertySignature, _context: PythonVisitorContext): OTree;
    methodSignature(_node: ts.MethodSignature, _context: PythonVisitorContext): OTree;
    asExpression(node: ts.AsExpression, context: PythonVisitorContext): OTree;
    stringLiteral(node: ts.StringLiteral | ts.NoSubstitutionTemplateLiteral, _context: PythonVisitorContext): OTree;
    templateExpression(node: ts.TemplateExpression, context: PythonVisitorContext): OTree;
    maskingVoidExpression(node: ts.VoidExpression, _context: PythonVisitorContext): OTree;
    /**
     * Convert parameters
     *
     * If the last one has the type of a known struct, explode to keyword-only arguments.
     *
     * Returns a pair of [decls, excploded-var-name].
     */
    private convertFunctionCallParameters;
    /**
     * Convert arguments.
     *
     * If the last argument:
     *
     * - is an object literal, explode it.
     * - is itself an exploded argument in our call signature, explode the fields
     */
    private convertFunctionCallArguments;
    /**
     * Render a type.
     *
     * Not usually a thing in Python, but useful for declared variables.
     */
    private renderType;
    private addImport;
    /**
     * Find the import for the FQNs submodule, and return it and the rest of the name
     */
    private importedNameForType;
    private renderSyntheticImports;
}
export {};
//# sourceMappingURL=python.d.ts.map