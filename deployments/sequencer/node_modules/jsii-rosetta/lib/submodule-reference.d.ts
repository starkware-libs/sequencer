import * as ts from 'typescript';
export type SubmoduleReferenceMap = ReadonlyMap<ts.PropertyAccessExpression | ts.LeftHandSideExpression | ts.Identifier | ts.PrivateIdentifier, SubmoduleReference>;
export declare class SubmoduleReference {
    readonly root: ts.Symbol;
    readonly submoduleChain: ts.LeftHandSideExpression | ts.Identifier | ts.PrivateIdentifier;
    readonly path: readonly ts.Node[];
    static inSourceFile(sourceFile: ts.SourceFile, typeChecker: ts.TypeChecker): SubmoduleReferenceMap;
    private static inNode;
    private constructor();
    get lastNode(): ts.Node;
    toString(): string;
}
//# sourceMappingURL=submodule-reference.d.ts.map