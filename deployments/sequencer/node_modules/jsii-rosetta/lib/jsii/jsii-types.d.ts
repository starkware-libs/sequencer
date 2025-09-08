import * as ts from 'typescript';
import { JsiiSymbol } from './jsii-utils';
import { BuiltInType } from '../typescript/types';
export type JsiiType = {
    kind: 'unknown';
} | {
    kind: 'error';
    message: string;
} | {
    kind: 'map' | 'list';
    elementType: JsiiType;
    elementTypeSymbol: ts.Symbol | undefined;
} | {
    kind: 'namedType';
    name: string;
} | {
    kind: 'builtIn';
    builtIn: BuiltInType;
};
export declare function determineJsiiType(typeChecker: ts.TypeChecker, type: ts.Type): JsiiType;
export type ObjectLiteralAnalysis = ObjectLiteralStruct | {
    readonly kind: 'map';
} | {
    readonly kind: 'unknown';
};
export type ObjectLiteralStruct = {
    readonly kind: 'struct';
    readonly type: ts.Type;
    readonly jsiiSym: JsiiSymbol;
} | {
    readonly kind: 'local-struct';
    readonly type: ts.Type;
};
export declare function analyzeObjectLiteral(typeChecker: ts.TypeChecker, node: ts.ObjectLiteralExpression): ObjectLiteralAnalysis;
//# sourceMappingURL=jsii-types.d.ts.map