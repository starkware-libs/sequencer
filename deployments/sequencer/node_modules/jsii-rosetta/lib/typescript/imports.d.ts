import * as ts from 'typescript';
import { JsiiSymbol } from '../jsii/jsii-utils';
import { AstRenderer } from '../renderer';
import { SubmoduleReferenceMap } from '../submodule-reference';
/**
 * Our own unification of import statements
 */
export interface ImportStatement {
    readonly node: ts.Node;
    readonly packageName: string;
    readonly imports: FullImport | SelectiveImport;
    readonly moduleSymbol?: JsiiSymbol;
}
export type FullImport = {
    readonly import: 'full';
    /**
     * The name of the namespace prefix in the source code. Used to strip the
     * prefix in certain languages (e.g: Java).
     */
    readonly sourceName: string;
    /**
     * The name under which this module is imported. Undefined if the module is
     * not aliased (could be the case for namepsace/submodule imports).
     */
    readonly alias?: string;
};
export type SelectiveImport = {
    readonly import: 'selective';
    readonly elements: ImportBinding[];
};
export interface ImportBinding {
    readonly sourceName: string;
    readonly alias?: string;
    /**
     * The JSII Symbol the import refers to
     */
    readonly importedSymbol?: JsiiSymbol;
}
export declare function analyzeImportEquals(node: ts.ImportEqualsDeclaration, context: AstRenderer<any>): ImportStatement;
export declare function analyzeImportDeclaration(node: ts.ImportDeclaration | ts.JSDocImportTag, context: AstRenderer<any>): ImportStatement;
export declare function analyzeImportDeclaration(node: ts.ImportDeclaration | ts.JSDocImportTag, context: AstRenderer<any>, submoduleReferences: SubmoduleReferenceMap): ImportStatement[];
//# sourceMappingURL=imports.d.ts.map