"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.firstTypeInUnion = firstTypeInUnion;
exports.builtInTypeName = builtInTypeName;
exports.renderType = renderType;
exports.parameterAcceptsUndefined = parameterAcceptsUndefined;
exports.typeContainsUndefined = typeContainsUndefined;
exports.renderTypeFlags = renderTypeFlags;
exports.mapElementType = mapElementType;
exports.inferMapElementType = inferMapElementType;
exports.arrayElementType = arrayElementType;
exports.typeOfExpression = typeOfExpression;
exports.inferredTypeOfExpression = inferredTypeOfExpression;
exports.isNumber = isNumber;
exports.isEnumAccess = isEnumAccess;
exports.isStaticReadonlyAccess = isStaticReadonlyAccess;
exports.renderFlags = renderFlags;
exports.determineReturnType = determineReturnType;
const ts = require("typescript");
const jsii_utils_1 = require("../jsii/jsii-utils");
const util_1 = require("../util");
/**
 * Return the first non-undefined type from a union
 */
function firstTypeInUnion(typeChecker, type) {
    type = typeChecker.getNonNullableType(type);
    if (!type.isUnion()) {
        return type;
    }
    return type.types[0];
}
function builtInTypeName(type) {
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.Any | ts.TypeFlags.Unknown)) {
        return 'any';
    }
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.BooleanLike)) {
        return 'boolean';
    }
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.NumberLike)) {
        return 'number';
    }
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.StringLike)) {
        return 'string';
    }
    return undefined;
}
function renderType(type) {
    if (type.isClassOrInterface()) {
        return type.symbol.name;
    }
    if (type.isLiteral()) {
        // eslint-disable-next-line @typescript-eslint/restrict-template-expressions
        return `${type.value}`;
    }
    return renderTypeFlags(type);
}
function parameterAcceptsUndefined(param, type) {
    if (param.initializer !== undefined) {
        return true;
    }
    if (param.questionToken !== undefined) {
        return true;
    }
    if (type) {
        return typeContainsUndefined(type);
    }
    return false;
}
/**
 * This is a simplified check that should be good enough for most purposes
 */
function typeContainsUndefined(type) {
    if (type.getFlags() & ts.TypeFlags.Undefined) {
        return true;
    }
    if (type.isUnion()) {
        return type.types.some(typeContainsUndefined);
    }
    return false;
}
function renderTypeFlags(type) {
    return renderFlags(type.flags, ts.TypeFlags);
}
/**
 * If this is a map type, return the type mapped *to* (key must always be `string` anyway).
 */
function mapElementType(type, typeChecker) {
    if ((0, jsii_utils_1.hasAnyFlag)(type.flags, ts.TypeFlags.Object) && type.symbol) {
        if (type.symbol.name === '__type') {
            // Declared map type: {[k: string]: A}
            return { result: 'map', elementType: type.getStringIndexType() };
        }
        if (type.symbol.name === '__object') {
            // Derived map type from object literal: typeof({ k: "value" })
            // For every property, get the node that created it (PropertyAssignment), and get the type of the initializer of that node
            const initializerTypes = type.getProperties().map((p) => {
                const expression = p.valueDeclaration ?? p.declarations[0];
                return typeOfObjectLiteralProperty(typeChecker, expression);
            });
            return {
                result: 'map',
                elementType: typeIfSame([...initializerTypes, type.getStringIndexType()].filter(util_1.isDefined)),
            };
        }
    }
    return { result: 'nonMap' };
}
/**
 * Try to infer the map element type from the properties if they're all the same
 */
function inferMapElementType(elements, typeChecker) {
    const types = elements.map((e) => typeOfObjectLiteralProperty(typeChecker, e)).filter(util_1.isDefined);
    return types.every((t) => isSameType(types[0], t)) ? types[0] : undefined;
}
function typeOfObjectLiteralProperty(typeChecker, el) {
    if (ts.isPropertyAssignment(el)) {
        return typeOfExpression(typeChecker, el.initializer);
    }
    if (ts.isShorthandPropertyAssignment(el)) {
        return typeOfExpression(typeChecker, el.name);
    }
    return undefined;
}
function isSameType(a, b) {
    return a.flags === b.flags && a.symbol?.name === b.symbol?.name;
}
function typeIfSame(types) {
    const ttypes = types.filter(util_1.isDefined);
    if (types.length === 0) {
        return undefined;
    }
    return ttypes.every((t) => isSameType(ttypes[0], t)) ? ttypes[0] : undefined;
}
/**
 * If this is an array type, return the element type of the array
 */
function arrayElementType(type) {
    if (type.symbol && type.symbol.name === 'Array') {
        const tr = type;
        return tr.aliasTypeArguments && tr.aliasTypeArguments[0];
    }
    return undefined;
}
function typeOfExpression(typeChecker, node) {
    const t = typeChecker.getContextualType(node) ?? typeChecker.getTypeAtLocation(node);
    return (0, jsii_utils_1.resolveEnumLiteral)(typeChecker, t);
}
/**
 * Infer type of expression by the argument it is assigned to
 *
 * If the type of the expression can include undefined (if the value is
 * optional), `undefined` will be removed from the union.
 *
 * (Will return undefined for object literals not unified with a declared type)
 */
function inferredTypeOfExpression(typeChecker, node) {
    const type = typeChecker.getContextualType(node);
    return type ? typeChecker.getNonNullableType(type) : undefined;
}
function isNumber(x) {
    return typeof x === 'number';
}
function isEnumAccess(typeChecker, access) {
    const symbol = (0, jsii_utils_1.resolvedSymbolAtLocation)(typeChecker, access.expression);
    return symbol ? (0, jsii_utils_1.hasAnyFlag)(symbol.flags, ts.SymbolFlags.Enum) : false;
}
function isStaticReadonlyAccess(typeChecker, access) {
    const symbol = (0, jsii_utils_1.resolvedSymbolAtLocation)(typeChecker, access);
    const decl = symbol?.getDeclarations();
    if (decl && decl[0] && ts.isPropertyDeclaration(decl[0])) {
        const flags = ts.getCombinedModifierFlags(decl[0]);
        return (0, jsii_utils_1.hasAllFlags)(flags, ts.ModifierFlags.Readonly | ts.ModifierFlags.Static);
    }
    return false;
}
function renderFlags(flags, flagObject) {
    if (flags === undefined) {
        return '';
    }
    return Object.values(flagObject)
        .filter(isNumber)
        .filter((f) => (0, jsii_utils_1.hasAllFlags)(flags, f))
        .map((f) => flagObject[f])
        .join(',');
}
function determineReturnType(typeChecker, node) {
    const signature = typeChecker.getSignatureFromDeclaration(node);
    if (!signature) {
        return undefined;
    }
    return typeChecker.getReturnTypeOfSignature(signature);
}
//# sourceMappingURL=types.js.map