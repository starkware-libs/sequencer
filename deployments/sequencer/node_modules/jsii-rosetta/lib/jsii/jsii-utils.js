"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.isNamedLikeStruct = isNamedLikeStruct;
exports.analyzeStructType = analyzeStructType;
exports.isJsiiProtocolType = isJsiiProtocolType;
exports.hasAllFlags = hasAllFlags;
exports.hasAnyFlag = hasAnyFlag;
exports.propertiesOfStruct = propertiesOfStruct;
exports.structPropertyAcceptsUndefined = structPropertyAcceptsUndefined;
exports.lookupJsiiSymbolFromNode = lookupJsiiSymbolFromNode;
exports.resolveJsiiSymbolType = resolveJsiiSymbolType;
exports.lookupJsiiSymbol = lookupJsiiSymbol;
exports.resolveEnumLiteral = resolveEnumLiteral;
exports.resolvedSymbolAtLocation = resolvedSymbolAtLocation;
exports.parentSymbol = parentSymbol;
exports.simpleName = simpleName;
exports.namespaceName = namespaceName;
const spec = require("@jsii/spec");
const common_1 = require("jsii/common");
const ts = require("typescript");
const assemblies_1 = require("./assemblies");
const types_1 = require("../typescript/types");
const util_1 = require("../util");
function isNamedLikeStruct(name) {
    // Start with an I and another uppercase character
    return !/^I[A-Z]/.test(name);
}
function analyzeStructType(typeChecker, type) {
    if (!type.isClassOrInterface() ||
        !hasAllFlags(type.objectFlags, ts.ObjectFlags.Interface) ||
        !isNamedLikeStruct(type.symbol.name)) {
        return false;
    }
    const jsiiSym = lookupJsiiSymbol(typeChecker, type.symbol);
    if (jsiiSym) {
        return { kind: 'struct', type, jsiiSym };
    }
    return { kind: 'local-struct', type };
}
/**
 * Whether the given type is a protocol AND comes from jsii
 *
 * - Protocol: a TypeScript interface that is *not* a "struct" type.
 *   A.k.a. "behavioral interface".
 * - From jsii: whether the interface type is defined in and exported
 *   via a jsii assembly. There can be literal interfaces defined
 *   in an example, and they will not be mangled in the same way
 *   as a jsii interface would be.
 *
 *
 * Examples:
 *
 * ```ts
 * // isJsiiProtocolType() -> false: not a protocol
 * interface Banana {
 *   readonly arc: number;
 * }
 *
 * // isJsiiProtocolType() -> might be true: depends on whether it was defined
 * // in a jsii assembly.
 * interface IHello {
 *   sayIt(): void;
 * }
 *
 * // isJsiiProtocolType() -> false: declared to not be a protocol, even though
 * // it has the naming scheme of one
 * /**
 *  * @struct
 *  * /
 * interface IPAddress {
 *   readonly octets: number[];
 * }
 * ```
 */
function isJsiiProtocolType(typeChecker, type) {
    if (!type.isClassOrInterface() || !hasAllFlags(type.objectFlags, ts.ObjectFlags.Interface)) {
        return false;
    }
    const sym = lookupJsiiSymbol(typeChecker, type.symbol);
    if (!sym) {
        return false;
    }
    if (!sym.sourceAssembly) {
        // No source assembly, so this is a 'fake-from-jsii' type
        return !isNamedLikeStruct(type.symbol.name);
    }
    const jsiiType = resolveJsiiSymbolType(sym);
    return spec.isInterfaceType(jsiiType) && !jsiiType.datatype;
}
function hasAllFlags(flags, test) {
    // tslint:disable-next-line:no-bitwise
    return test !== 0 && (flags & test) === test;
}
function hasAnyFlag(flags, test) {
    // tslint:disable-next-line:no-bitwise
    return test !== 0 && (flags & test) !== 0;
}
function propertiesOfStruct(type, context) {
    return type.isClassOrInterface()
        ? type.getProperties().map((s) => {
            let propType;
            let questionMark = false;
            const propSymbol = type.getProperty(s.name);
            const symbolDecl = propSymbol.valueDeclaration ?? propSymbol.declarations[0];
            if (ts.isPropertyDeclaration(symbolDecl) || ts.isPropertySignature(symbolDecl)) {
                questionMark = symbolDecl.questionToken !== undefined;
                propType = symbolDecl.type && context.typeOfType(symbolDecl.type);
            }
            return {
                name: s.name,
                type: propType,
                questionMark,
            };
        })
        : [];
}
function structPropertyAcceptsUndefined(prop) {
    return prop.questionMark || (!!prop.type && (0, types_1.typeContainsUndefined)(prop.type));
}
function lookupJsiiSymbolFromNode(typeChecker, node) {
    return (0, util_1.fmap)(typeChecker.getSymbolAtLocation(node), (s) => lookupJsiiSymbol(typeChecker, s));
}
function resolveJsiiSymbolType(jsiiSymbol) {
    if (jsiiSymbol.symbolType !== 'type') {
        throw new Error(`Expected symbol to refer to a 'type', got '${jsiiSymbol.fqn}' which is a '${jsiiSymbol.symbolType}'`);
    }
    if (!jsiiSymbol.sourceAssembly) {
        throw new Error('`resolveJsiiSymbolType: requires an actual source assembly');
    }
    const type = jsiiSymbol.sourceAssembly?.assembly.types?.[jsiiSymbol.fqn];
    if (!type) {
        throw new Error(`resolveJsiiSymbolType: ${jsiiSymbol.fqn} not found in assembly ${jsiiSymbol.sourceAssembly.assembly.name}`);
    }
    return type;
}
/**
 * Returns the jsii FQN for a TypeScript (class or type) symbol
 *
 * TypeScript only knows the symbol NAME plus the FILE the symbol is defined
 * in. We need to extract two things:
 *
 * 1. The package name (extracted from the nearest `package.json`)
 * 2. The submodule name (...?? don't know how to get this yet)
 * 3. Any containing type names or namespace names.
 *
 * For tests, we also treat symbols in a file that has the string '/// fake-from-jsii'
 * as coming from jsii.
 */
function lookupJsiiSymbol(typeChecker, sym) {
    // Resolve alias, if it is one. This comes into play if the symbol refers to a module,
    // we need to resolve the alias to find the ACTUAL module.
    if (hasAnyFlag(sym.flags, ts.SymbolFlags.Alias)) {
        sym = typeChecker.getAliasedSymbol(sym);
    }
    const decl = sym.declarations?.[0];
    if (!decl) {
        return undefined;
    }
    if (ts.isSourceFile(decl)) {
        // This is a module.
        const sourceAssembly = (0, assemblies_1.findTypeLookupAssembly)(decl.fileName);
        return (0, util_1.fmap)(sourceAssembly, (asm) => ({
            fqn: (0, util_1.fmap)((0, common_1.symbolIdentifier)(typeChecker, sym, (0, util_1.fmap)(sourceAssembly, (sa) => ({ assembly: sa.assembly }))), (symbolId) => sourceAssembly?.symbolIdMap[symbolId]) ?? sourceAssembly?.assembly.name,
            sourceAssembly: asm,
            symbolType: 'module',
        }));
    }
    if (!isDeclaration(decl)) {
        return undefined;
    }
    const declaringFile = decl.getSourceFile();
    if (/^\/\/\/ fake-from-jsii/m.test(declaringFile.getFullText())) {
        return { fqn: `fake_jsii.${sym.name}`, symbolType: 'type' };
    }
    const declSym = getSymbolFromDeclaration(decl, typeChecker);
    if (!declSym) {
        return undefined;
    }
    const fileName = decl.getSourceFile().fileName;
    const sourceAssembly = (0, assemblies_1.findTypeLookupAssembly)(fileName);
    const symbolId = (0, common_1.symbolIdentifier)(typeChecker, declSym, { assembly: sourceAssembly?.assembly });
    if (!symbolId) {
        return undefined;
    }
    return (0, util_1.fmap)(/([^#]*)(#.*)?/.exec(symbolId), ([, typeSymbolId, memberFragment]) => {
        if (memberFragment) {
            return (0, util_1.fmap)(sourceAssembly?.symbolIdMap[typeSymbolId], (fqn) => ({
                fqn: `${fqn}${memberFragment}`,
                sourceAssembly,
                symbolType: 'member',
            }));
        }
        return (0, util_1.fmap)(sourceAssembly?.symbolIdMap[typeSymbolId], (fqn) => ({ fqn, sourceAssembly, symbolType: 'type' }));
    });
}
function isDeclaration(x) {
    return (ts.isClassDeclaration(x) ||
        ts.isNamespaceExportDeclaration(x) ||
        ts.isNamespaceExport(x) ||
        ts.isModuleDeclaration(x) ||
        ts.isEnumDeclaration(x) ||
        ts.isEnumMember(x) ||
        ts.isInterfaceDeclaration(x) ||
        ts.isMethodDeclaration(x) ||
        ts.isMethodSignature(x) ||
        ts.isPropertyDeclaration(x) ||
        ts.isPropertySignature(x));
}
/**
 * If the given type is an enum literal, resolve to the enum type
 */
function resolveEnumLiteral(typeChecker, type) {
    if (!hasAnyFlag(type.flags, ts.TypeFlags.EnumLiteral)) {
        return type;
    }
    return typeChecker.getBaseTypeOfLiteralType(type);
}
function resolvedSymbolAtLocation(typeChecker, node) {
    let symbol = typeChecker.getSymbolAtLocation(node);
    while (symbol && hasAnyFlag(symbol.flags, ts.SymbolFlags.Alias)) {
        symbol = typeChecker.getAliasedSymbol(symbol);
    }
    return symbol;
}
function getSymbolFromDeclaration(decl, typeChecker) {
    if (!isDeclaration(decl)) {
        return undefined;
    }
    const name = ts.getNameOfDeclaration(decl);
    return name ? typeChecker.getSymbolAtLocation(name) : undefined;
}
function parentSymbol(sym) {
    const parts = sym.fqn.split('.');
    if (parts.length === 1) {
        return undefined;
    }
    return {
        fqn: parts.slice(0, -1).join('.'),
        symbolType: 'module', // Might not be true, but probably good enough
        sourceAssembly: sym.sourceAssembly,
    };
}
/**
 * Get the last part of a dot-separated string
 */
function simpleName(x) {
    return x.split('.').slice(-1)[0];
}
/**
 * Get all parts except the last of a dot-separated string
 */
function namespaceName(x) {
    return x.split('.').slice(0, -1).join('.');
}
//# sourceMappingURL=jsii-utils.js.map