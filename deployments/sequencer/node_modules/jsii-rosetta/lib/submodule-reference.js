"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.SubmoduleReference = void 0;
const ts = require("typescript");
class SubmoduleReference {
    static inSourceFile(sourceFile, typeChecker) {
        const importDeclarations = sourceFile.statements
            .filter((stmt) => ts.isImportDeclaration(stmt))
            .flatMap((stmt) => importedSymbolsFrom(stmt, sourceFile, typeChecker));
        return SubmoduleReference.inNode(sourceFile, typeChecker, new Set(importDeclarations));
    }
    static inNode(node, typeChecker, importDeclarations, map = new Map()) {
        if (ts.isPropertyAccessExpression(node)) {
            const [head, ...tail] = propertyPath(node);
            const symbol = typeChecker.getSymbolAtLocation(head.name);
            if (symbol && importDeclarations.has(symbol)) {
                // This is a reference within an imported namespace, so we need to record that...
                const firstNonNamespace = tail.findIndex((item) => !isLikelyNamespace(item.name, typeChecker));
                if (firstNonNamespace < 0) {
                    map.set(node.expression, new SubmoduleReference(symbol, node.expression, []));
                }
                else {
                    const tailEnd = tail[firstNonNamespace].expression;
                    const path = tail.slice(0, firstNonNamespace).map((item) => item.name);
                    map.set(tailEnd, new SubmoduleReference(symbol, tailEnd, path));
                }
            }
            return map;
        }
        // Faster than ||-ing a bung of if statements to avoid traversing uninteresting nodes...
        switch (node.kind) {
            case ts.SyntaxKind.ImportDeclaration:
            case ts.SyntaxKind.ExportDeclaration:
                break;
            default:
                for (const child of node.getChildren()) {
                    map = SubmoduleReference.inNode(child, typeChecker, importDeclarations, map);
                }
        }
        return map;
    }
    constructor(root, submoduleChain, path) {
        this.root = root;
        this.submoduleChain = submoduleChain;
        this.path = path;
    }
    get lastNode() {
        if (this.path.length === 0) {
            const node = this.root.valueDeclaration ?? this.root.declarations[0];
            return ts.isNamespaceImport(node) || ts.isImportSpecifier(node) ? node.name : node;
        }
        return this.path[this.path.length - 1];
    }
    toString() {
        return `${this.constructor.name}<root=${this.root.name}, path=${JSON.stringify(this.path.map((item) => item.getText(item.getSourceFile())))}>`;
    }
}
exports.SubmoduleReference = SubmoduleReference;
/**
 * Determines what symbols are imported by the given TypeScript import
 * delcaration, in the context of the specified file, using the provided type
 * checker.
 *
 * @param decl        an import declaration.
 * @param sourceFile  the source file that contains the import declaration.
 * @param typeChecker a TypeChecker instance valid for the provided source file.
 *
 * @returns the (possibly empty) list of symbols imported by this declaration.
 */
function importedSymbolsFrom(decl, sourceFile, typeChecker) {
    const { importClause } = decl;
    if (importClause == null) {
        // This is a "for side effects" import, which isn't relevant for our business here...
        return [];
    }
    const { name, namedBindings } = importClause;
    const imports = new Array();
    if (name != null) {
        const symbol = typeChecker.getSymbolAtLocation(name);
        if (symbol == null) {
            throw new Error(`No symbol was defined for node ${name.getText(sourceFile)}`);
        }
        imports.push(symbol);
    }
    if (namedBindings != null) {
        if (ts.isNamespaceImport(namedBindings)) {
            const { name: bindingName } = namedBindings;
            const symbol = typeChecker.getSymbolAtLocation(bindingName);
            if (symbol == null) {
                throw new Error(`No symbol was defined for node ${bindingName.getText(sourceFile)}`);
            }
            imports.push(symbol);
        }
        else {
            for (const specifier of namedBindings.elements) {
                const { name: specifierName } = specifier;
                const symbol = typeChecker.getSymbolAtLocation(specifierName);
                if (symbol == null) {
                    throw new Error(`No symbol was defined for node ${specifierName.getText(sourceFile)}`);
                }
                imports.push(symbol);
            }
        }
    }
    return imports;
}
function propertyPath(node) {
    const { expression, name } = node;
    if (!ts.isPropertyAccessExpression(expression)) {
        return [
            { name: expression, expression },
            { name, expression },
        ];
    }
    return [...propertyPath(expression), { name, expression }];
}
/**
 * A heuristic to determine whether the provided node likely refers to some
 * namespace.
 *
 * @param node        the node to be checked.
 * @param typeChecker a type checker that can obtain symbols for this node.
 *
 * @returns true if the node likely refers to a namespace name.
 */
function isLikelyNamespace(node, typeChecker) {
    if (!ts.isIdentifier(node)) {
        return false;
    }
    // If the identifier was bound to a symbol, we can inspect the declarations of
    // it to validate they are all module or namespace declarations.
    const symbol = typeChecker.getSymbolAtLocation(node);
    if (symbol != null) {
        return (symbol.declarations.length > 0 &&
            symbol.declarations.every((decl) => ts.isModuleDeclaration(decl) || ts.isNamespaceExport(decl) || ts.isNamespaceImport(decl)));
    }
    // We understand this is likely a namespace if the name does not start with
    // upper-case letter.
    return !startsWithUpperCase(node.text);
}
function startsWithUpperCase(text) {
    return text.length > 0 && text[0] === text[0].toUpperCase();
}
//# sourceMappingURL=submodule-reference.js.map