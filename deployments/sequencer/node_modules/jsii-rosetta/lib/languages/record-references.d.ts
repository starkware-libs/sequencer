import * as ts from 'typescript';
import { DefaultVisitor } from './default';
import { TargetLanguage } from '../languages/target-language';
import { OTree } from '../o-tree';
import { AstRenderer } from '../renderer';
import { SubmoduleReference } from '../submodule-reference';
import { Spans } from '../typescript/visible-spans';
interface RecordReferencesContext {
}
type RecordReferencesRenderer = AstRenderer<RecordReferencesContext>;
/**
 * A visitor that collects all types referenced in a particular piece of sample code
 */
export declare class RecordReferencesVisitor extends DefaultVisitor<RecordReferencesContext> {
    private readonly visibleSpans;
    static readonly VERSION = "2";
    readonly language = TargetLanguage.PYTHON;
    readonly defaultContext: {};
    private readonly references;
    constructor(visibleSpans: Spans);
    fqnsReferenced(): string[];
    mergeContext(old: RecordReferencesContext, update: Partial<RecordReferencesContext>): RecordReferencesContext;
    /**
     * For a variable declaration, a type counts as "referenced" if it gets assigned a value via an initializer
     *
     * This skips "declare" statements which aren't really interesting.
     */
    variableDeclaration(node: ts.VariableDeclaration, renderer: RecordReferencesRenderer): OTree;
    newExpression(node: ts.NewExpression, context: RecordReferencesRenderer): OTree;
    propertyAccessExpression(node: ts.PropertyAccessExpression, context: RecordReferencesRenderer, submoduleReference: SubmoduleReference | undefined): OTree;
    regularCallExpression(node: ts.CallExpression, context: RecordReferencesRenderer): OTree;
    objectLiteralExpression(node: ts.ObjectLiteralExpression, context: RecordReferencesRenderer): OTree;
    propertyAssignment(node: ts.PropertyAssignment, renderer: RecordReferencesRenderer): OTree;
    shorthandPropertyAssignment(node: ts.ShorthandPropertyAssignment, renderer: RecordReferencesRenderer): OTree;
    /**
     * Visit the arguments by type (instead of by node)
     *
     * This will make sure we recognize the use of a `BucketProps` in a `new Bucket(..., { ... })` call.
     */
    private visitArgumentTypes;
    private recordNode;
    private recordSymbol;
}
export {};
//# sourceMappingURL=record-references.d.ts.map