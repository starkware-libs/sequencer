import * as ts from 'typescript';
/**
 * Return the first non-undefined type from a union
 */
export declare function firstTypeInUnion(typeChecker: ts.TypeChecker, type: ts.Type): ts.Type;
export type BuiltInType = 'any' | 'boolean' | 'number' | 'string' | 'void';
export declare function builtInTypeName(type: ts.Type): BuiltInType | undefined;
export declare function renderType(type: ts.Type): string;
export declare function parameterAcceptsUndefined(param: ts.ParameterDeclaration, type?: ts.Type): boolean;
/**
 * This is a simplified check that should be good enough for most purposes
 */
export declare function typeContainsUndefined(type: ts.Type): boolean;
export declare function renderTypeFlags(type: ts.Type): string;
export type MapAnalysis = {
    result: 'nonMap';
} | {
    result: 'map';
    elementType: ts.Type | undefined;
};
/**
 * If this is a map type, return the type mapped *to* (key must always be `string` anyway).
 */
export declare function mapElementType(type: ts.Type, typeChecker: ts.TypeChecker): MapAnalysis;
/**
 * Try to infer the map element type from the properties if they're all the same
 */
export declare function inferMapElementType(elements: readonly ts.ObjectLiteralElementLike[], typeChecker: ts.TypeChecker): ts.Type | undefined;
/**
 * If this is an array type, return the element type of the array
 */
export declare function arrayElementType(type: ts.Type): ts.Type | undefined;
export declare function typeOfExpression(typeChecker: ts.TypeChecker, node: ts.Expression): ts.Type;
/**
 * Infer type of expression by the argument it is assigned to
 *
 * If the type of the expression can include undefined (if the value is
 * optional), `undefined` will be removed from the union.
 *
 * (Will return undefined for object literals not unified with a declared type)
 */
export declare function inferredTypeOfExpression(typeChecker: ts.TypeChecker, node: ts.Expression): ts.Type | undefined;
export declare function isNumber(x: any): x is number;
export declare function isEnumAccess(typeChecker: ts.TypeChecker, access: ts.PropertyAccessExpression): boolean;
export declare function isStaticReadonlyAccess(typeChecker: ts.TypeChecker, access: ts.PropertyAccessExpression): boolean;
export declare function renderFlags(flags: number | undefined, flagObject: Record<string, number | string>): string;
export declare function determineReturnType(typeChecker: ts.TypeChecker, node: ts.SignatureDeclaration): ts.Type | undefined;
//# sourceMappingURL=types.d.ts.map