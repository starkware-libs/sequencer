"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.analyzeImportEquals = analyzeImportEquals;
exports.analyzeImportDeclaration = analyzeImportDeclaration;
const ts = require("typescript");
const ast_utils_1 = require("./ast-utils");
const jsii_utils_1 = require("../jsii/jsii-utils");
const util_1 = require("../util");
function analyzeImportEquals(node, context) {
    let moduleName = '???';
    (0, ast_utils_1.matchAst)(node.moduleReference, (0, ast_utils_1.nodeOfType)('ref', ts.SyntaxKind.ExternalModuleReference), (bindings) => {
        moduleName = (0, ast_utils_1.stringFromLiteral)(bindings.ref.expression);
    });
    const sourceName = context.textOf(node.name);
    return {
        node,
        packageName: moduleName,
        moduleSymbol: (0, jsii_utils_1.lookupJsiiSymbolFromNode)(context.typeChecker, node.name),
        imports: { import: 'full', alias: sourceName, sourceName },
    };
}
function analyzeImportDeclaration(node, context, submoduleReferences) {
    const packageName = (0, ast_utils_1.stringFromLiteral)(node.moduleSpecifier);
    const starBindings = (0, ast_utils_1.matchAst)(node, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.ImportDeclaration, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.ImportClause, (0, ast_utils_1.nodeOfType)('namespace', ts.SyntaxKind.NamespaceImport))));
    if (starBindings) {
        const sourceName = context.textOf(starBindings.namespace.name);
        const bareImport = {
            node,
            packageName,
            moduleSymbol: (0, jsii_utils_1.lookupJsiiSymbolFromNode)(context.typeChecker, starBindings.namespace.name),
            imports: {
                import: 'full',
                alias: sourceName,
                sourceName,
            },
        };
        if (submoduleReferences == null) {
            return bareImport;
        }
        const rootSymbol = context.typeChecker.getSymbolAtLocation(starBindings.namespace.name);
        const refs = rootSymbol && Array.from(submoduleReferences.values()).filter((ref) => ref.root === rootSymbol);
        // No submodule reference, or only 1 where the path is empty (this is used to signal the use of the bare import so it's not erased)
        if (refs == null || refs.length === 0 || (refs.length === 1 && refs[0].path.length === 0)) {
            return [bareImport];
        }
        return refs.flatMap(({ lastNode, path, root, submoduleChain }, idx, array) => {
            if (array
                .slice(0, idx)
                .some((other) => other.root === root && context.textOf(other.submoduleChain) === context.textOf(submoduleChain))) {
                // This would be a duplicate, so we're skipping it
                return [];
            }
            const moduleSymbol = (0, jsii_utils_1.lookupJsiiSymbolFromNode)(context.typeChecker, lastNode);
            return [
                {
                    node,
                    packageName: [packageName, ...path.map((n) => context.textOf(n))].join('/'),
                    moduleSymbol,
                    imports: {
                        import: 'full',
                        alias: undefined, // No alias exists in the source text for this...
                        sourceName: context.textOf(submoduleChain),
                    },
                },
            ];
        });
    }
    const namedBindings = (0, ast_utils_1.matchAst)(node, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.ImportDeclaration, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.ImportClause, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.NamedImports, (0, ast_utils_1.allOfType)(ts.SyntaxKind.ImportSpecifier, 'specifiers')))));
    const extraImports = new Array();
    const elements = (namedBindings?.specifiers ?? []).flatMap(({ name, propertyName }) => {
        // regular import { name }
        // renamed import { propertyName as name }
        const directBinding = {
            sourceName: context.textOf(propertyName ?? name),
            alias: propertyName && context.textOf(name),
            importedSymbol: (0, jsii_utils_1.lookupJsiiSymbolFromNode)(context.typeChecker, propertyName ?? name),
        };
        if (submoduleReferences != null) {
            const symbol = context.typeChecker.getSymbolAtLocation(name);
            let omitDirectBinding = false;
            for (const match of Array.from(submoduleReferences.values()).filter((ref) => ref.root === symbol)) {
                if (match.path.length === 0) {
                    // This is a namespace binding that is used as-is (not via a transitive path). It needs to be preserved.
                    omitDirectBinding = false;
                    continue;
                }
                const subPackageName = [packageName, ...match.path.map((n) => n.getText(n.getSourceFile()))].join('/');
                const importedSymbol = (0, jsii_utils_1.lookupJsiiSymbolFromNode)(context.typeChecker, match.lastNode);
                const moduleSymbol = (0, util_1.fmap)(importedSymbol, jsii_utils_1.parentSymbol);
                const importStatement = extraImports.find((stmt) => {
                    if (moduleSymbol != null) {
                        return stmt.moduleSymbol === moduleSymbol;
                    }
                    return stmt.packageName === subPackageName;
                }) ??
                    extraImports[extraImports.push({
                        moduleSymbol,
                        node: match.lastNode,
                        packageName: subPackageName,
                        imports: { import: 'selective', elements: [] },
                    }) - 1];
                importStatement.imports.elements.push({
                    sourceName: context.textOf(match.submoduleChain),
                    importedSymbol,
                });
            }
            if (omitDirectBinding) {
                return [];
            }
        }
        return [directBinding];
    });
    if (submoduleReferences == null) {
        return {
            node,
            packageName,
            imports: { import: 'selective', elements },
            moduleSymbol: (0, util_1.fmap)(elements?.[0]?.importedSymbol, jsii_utils_1.parentSymbol),
        };
    }
    return [
        {
            node,
            packageName,
            imports: { import: 'selective', elements },
            moduleSymbol: (0, util_1.fmap)(elements?.[0]?.importedSymbol, jsii_utils_1.parentSymbol),
        },
        ...extraImports,
    ];
}
//# sourceMappingURL=imports.js.map