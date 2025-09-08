"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.determineJsiiType = determineJsiiType;
exports.analyzeObjectLiteral = analyzeObjectLiteral;
const ts = require("typescript");
const jsii_utils_1 = require("./jsii-utils");
const types_1 = require("../typescript/types");
function determineJsiiType(typeChecker, type) {
    // this means the snippet didn't have enough info for the TypeScript compiler to figure out the type -
    // so, just render the fallback
    if (type.intrinsicName === 'error') {
        return { kind: 'unknown' };
    }
    // The non-nullable version of `void` is `never`, so check first...
    if ((type.flags & (ts.TypeFlags.Void | ts.TypeFlags.VoidLike)) !== 0) {
        return { kind: 'builtIn', builtIn: 'void' };
    }
    // The non-nullable version of `unknown` is some ObjectType, so check first...
    if ((type.flags & (ts.TypeFlags.Unknown | ts.TypeFlags.Any)) !== 0) {
        return { kind: 'builtIn', builtIn: 'any' };
    }
    type = type.getNonNullableType();
    const mapValuesType = (0, types_1.mapElementType)(type, typeChecker);
    if (mapValuesType.result === 'map') {
        return {
            kind: 'map',
            elementType: mapValuesType.elementType
                ? determineJsiiType(typeChecker, mapValuesType.elementType)
                : { kind: 'builtIn', builtIn: 'any' },
            elementTypeSymbol: mapValuesType.elementType?.symbol,
        };
    }
    if (type.symbol?.name === 'Array') {
        const typeRef = type;
        if (typeRef.typeArguments?.length === 1) {
            return {
                kind: 'list',
                elementType: determineJsiiType(typeChecker, typeRef.typeArguments[0]),
                elementTypeSymbol: typeRef.typeArguments[0].symbol,
            };
        }
        return {
            kind: 'list',
            elementType: { kind: 'builtIn', builtIn: 'any' },
            elementTypeSymbol: undefined,
        };
    }
    // User-defined or aliased type
    if (type.aliasSymbol) {
        return { kind: 'namedType', name: type.aliasSymbol.name };
    }
    if (type.symbol) {
        return { kind: 'namedType', name: type.symbol.name };
    }
    const typeScriptBuiltInType = (0, types_1.builtInTypeName)(type);
    if (typeScriptBuiltInType) {
        return { kind: 'builtIn', builtIn: typeScriptBuiltInType };
    }
    if (type.isUnion() || type.isIntersection()) {
        return {
            kind: 'error',
            message: `Type unions or intersections are not supported in examples, got: ${typeChecker.typeToString(type)}`,
        };
    }
    return { kind: 'unknown' };
}
function analyzeObjectLiteral(typeChecker, node) {
    const type = (0, types_1.inferredTypeOfExpression)(typeChecker, node);
    if (!type) {
        return { kind: 'unknown' };
    }
    const call = findEnclosingCallExpression(node);
    const isDeclaredCall = !!(call && typeChecker.getResolvedSignature(call)?.declaration);
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.Any)) {
        // The type checker by itself won't tell us the difference between an `any` that
        // was literally declared as a type in the code, vs an `any` it assumes because it
        // can't find a function's type declaration.
        //
        // Search for the function's declaration and only if we can't find it,
        // the type is actually unknown (otherwise it's a literal 'any').
        return isDeclaredCall ? { kind: 'map' } : { kind: 'unknown' };
    }
    // If the type is a union between a struct and something else, return the first possible struct
    const structCandidates = type.isUnion() ? type.types : [type];
    for (const candidate of structCandidates) {
        const structType = (0, jsii_utils_1.analyzeStructType)(typeChecker, candidate);
        if (structType) {
            return structType;
        }
    }
    return { kind: 'map' };
}
function findEnclosingCallExpression(node) {
    while (node) {
        if (ts.isCallLikeExpression(node)) {
            return node;
        }
        node = node.parent;
    }
    return undefined;
}
//# sourceMappingURL=jsii-types.js.map