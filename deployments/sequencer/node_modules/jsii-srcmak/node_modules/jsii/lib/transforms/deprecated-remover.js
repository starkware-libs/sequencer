"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DeprecatedRemover = void 0;
const node_path_1 = require("node:path");
const spec_1 = require("@jsii/spec");
const ts = require("typescript");
const jsii_diagnostic_1 = require("../jsii-diagnostic");
const bindings = require("../node-bindings");
const utils_1 = require("../utils");
class DeprecatedRemover {
    constructor(typeChecker, allowlistedDeprecations) {
        this.typeChecker = typeChecker;
        this.allowlistedDeprecations = allowlistedDeprecations;
        this.transformations = new Array();
        this.nodesToRemove = new Set();
    }
    /**
     * Obtains the configuration for the TypeScript transform(s) that will remove
     * `@deprecated` members from the generated declarations (`.d.ts`) files. It
     * will leverage information accumulated during `#removeFrom(Assembly)` in
     * order to apply corrections to inheritance chains, ensuring a valid output
     * is produced.
     */
    get customTransformers() {
        return {
            afterDeclarations: [
                (context) => {
                    const transformer = new DeprecationRemovalTransformer(this.typeChecker, context, this.transformations, this.nodesToRemove);
                    return transformer.transform.bind(transformer);
                },
            ],
        };
    }
    /**
     * Removes all `@deprecated` API elements from the provided assembly, and
     * records the operations needed in order to fix the inheritance chains that
     * mix `@deprecated` and non-`@deprecated` types.
     *
     * @param assembly the assembly to be modified.
     *
     * @returns diagnostic messages produced when validating no remaining API
     *          makes use of a `@deprecated` type that was removed.
     */
    removeFrom(assembly) {
        if (assembly.types == null) {
            return [];
        }
        const strippedFqns = new Set();
        const replaceWithClass = new Map();
        const replaceWithInterfaces = new Map();
        // Find all types that will be stripped out
        for (const [fqn, typeInfo] of Object.entries(assembly.types)) {
            if (typeInfo.docs?.stability === spec_1.Stability.Deprecated) {
                if (!this.shouldFqnBeStripped(fqn)) {
                    continue;
                }
                strippedFqns.add(fqn);
                if ((0, spec_1.isClassType)(typeInfo) && typeInfo.base != null) {
                    replaceWithClass.set(fqn, typeInfo.base);
                }
                if ((0, spec_1.isClassOrInterfaceType)(typeInfo) && typeInfo.interfaces != null) {
                    replaceWithInterfaces.set(fqn, typeInfo.interfaces);
                }
                this.nodesToRemove.add(bindings.getRelatedNode(typeInfo));
            }
        }
        for (const [fqn, typeInfo] of Object.entries(assembly.types)) {
            // Ignore `@deprecated` types
            if (strippedFqns.has(fqn)) {
                continue;
            }
            // Enums cannot have references to `@deprecated` types, but can have deprecated members
            if ((0, spec_1.isEnumType)(typeInfo)) {
                const enumNode = bindings.getEnumRelatedNode(typeInfo);
                const members = [];
                for (const mem of typeInfo.members) {
                    if (mem.docs?.stability === spec_1.Stability.Deprecated && this.shouldFqnBeStripped(`${fqn}#${mem.name}`)) {
                        const matchingMemberNode = enumNode.members.find((enumMem) => enumMem.name.getText() === mem.name);
                        if (matchingMemberNode) {
                            this.nodesToRemove.add(matchingMemberNode);
                        }
                    }
                    else {
                        members.push(mem);
                    }
                }
                typeInfo.members = members;
                continue;
            }
            // For classes, we erase `@deprecated` base classes, replacing as needed
            const additionalInterfaces = new Set();
            if ((0, spec_1.isClassType)(typeInfo) && typeInfo.base != null && strippedFqns.has(typeInfo.base)) {
                while (typeInfo.base != null && strippedFqns.has(typeInfo.base)) {
                    const oldBase = assembly.types[typeInfo.base];
                    if (oldBase.interfaces)
                        for (const addFqn of oldBase.interfaces)
                            additionalInterfaces.add(addFqn);
                    typeInfo.base = replaceWithClass.get(typeInfo.base);
                }
                this.transformations.push(typeInfo.base != null
                    ? Transformation.replaceBaseClass(this.typeChecker, bindings.getClassRelatedNode(typeInfo), typeInfo.base in assembly.types
                        ? bindings.getClassRelatedNode(assembly.types[typeInfo.base]) ?? typeInfo.base
                        : typeInfo.base)
                    : Transformation.removeBaseClass(this.typeChecker, bindings.getClassRelatedNode(typeInfo)));
            }
            // Be defensive in case we add other kinds in the future
            if (!(0, spec_1.isClassOrInterfaceType)(typeInfo)) {
                throw new Error(`Unhandled type encountered! ${JSON.stringify(typeInfo, null, 2)}`);
            }
            // Strip all `@deprecated` interfaces from the inheritance tree, replacing as needed
            if (typeInfo.interfaces?.some((addFqn) => strippedFqns.has(addFqn)) || additionalInterfaces.size > 0) {
                const originalSet = new Set(typeInfo.interfaces ?? []);
                const newSet = new Set();
                const candidates = Array.from(new Set([...originalSet, ...additionalInterfaces]));
                while (candidates.length > 0) {
                    const candidateFqn = candidates.pop();
                    if (!strippedFqns.has(candidateFqn)) {
                        newSet.add(candidateFqn);
                        if (!originalSet.has(candidateFqn)) {
                            this.transformations.push(Transformation.addInterface(this.typeChecker, bindings.getClassOrInterfaceRelatedNode(typeInfo), candidateFqn in assembly.types
                                ? bindings.getInterfaceRelatedNode(assembly.types[candidateFqn]) ?? candidateFqn
                                : candidateFqn));
                        }
                        continue;
                    }
                    if (originalSet.has(candidateFqn)) {
                        this.transformations.push(Transformation.removeInterface(this.typeChecker, bindings.getClassOrInterfaceRelatedNode(typeInfo), bindings.getInterfaceRelatedNode(assembly.types[candidateFqn])));
                    }
                    const replacement = replaceWithInterfaces.get(candidateFqn);
                    if (replacement != null) {
                        candidates.push(...replacement);
                    }
                }
                typeInfo.interfaces = newSet.size > 0 ? Array.from(newSet).sort() : undefined;
            }
            // Drop all `@deprecated` members, and remove "overrides" from stripped types
            const methods = [];
            const properties = [];
            if (typeInfo.methods) {
                for (const meth of typeInfo.methods) {
                    if (meth.docs?.stability === spec_1.Stability.Deprecated && this.shouldFqnBeStripped(`${fqn}#${meth.name}`)) {
                        this.nodesToRemove.add(bindings.getMethodRelatedNode(meth));
                    }
                    else {
                        methods.push(meth.overrides != null && strippedFqns.has(meth.overrides) ? { ...meth, overrides: undefined } : meth);
                    }
                }
            }
            typeInfo.methods = typeInfo.methods ? methods : undefined;
            if (typeInfo.properties) {
                for (const prop of typeInfo.properties) {
                    if (prop.docs?.stability === spec_1.Stability.Deprecated && this.shouldFqnBeStripped(`${fqn}#${prop.name}`)) {
                        this.nodesToRemove.add(bindings.getParameterRelatedNode(prop));
                    }
                    else {
                        properties.push(prop.overrides != null && strippedFqns.has(prop.overrides) ? { ...prop, overrides: undefined } : prop);
                    }
                }
            }
            typeInfo.properties = typeInfo.properties ? properties : undefined;
        }
        const diagnostics = this.findLeftoverUseOfDeprecatedAPIs(assembly, strippedFqns);
        // Remove all `@deprecated` types, after we did everything, so we could
        // still access the related nodes from the assembly object.
        for (const fqn of strippedFqns) {
            if (this.shouldFqnBeStripped(fqn)) {
                delete assembly.types[fqn];
            }
        }
        return diagnostics;
    }
    findLeftoverUseOfDeprecatedAPIs(assembly, strippedFqns) {
        if (assembly.types == null) {
            return [];
        }
        const result = new Array();
        for (const type of Object.values(assembly.types)) {
            if ((0, spec_1.isEnumType)(type) || strippedFqns.has(type.fqn)) {
                continue;
            }
            if ((0, spec_1.isClassType)(type) && type.initializer) {
                result.push(...this.verifyCallable(assembly, strippedFqns, type.initializer));
            }
            if (type.methods) {
                for (const method of type.methods)
                    result.push(...this.verifyCallable(assembly, strippedFqns, method));
            }
            if (type.properties) {
                for (const property of type.properties)
                    result.push(...this.verifyProperty(assembly, strippedFqns, property));
            }
        }
        return result;
    }
    verifyCallable(assembly, strippedFqns, method) {
        const diagnostics = new Array();
        const deprecatedReturnFqn = (0, spec_1.isMethod)(method) && method.returns && this.tryFindReference(method.returns.type, strippedFqns);
        if (deprecatedReturnFqn) {
            diagnostics.push(this.makeDiagnostic(deprecatedReturnFqn, 'Method', method, assembly));
        }
        if (method.parameters) {
            for (const parameter of method.parameters) {
                const deprecatedTypeFqn = this.tryFindReference(parameter.type, strippedFqns);
                if (deprecatedTypeFqn) {
                    diagnostics.push(this.makeDiagnostic(deprecatedTypeFqn, 'Parameter', parameter, assembly));
                }
            }
        }
        return diagnostics;
    }
    verifyProperty(assembly, strippedFqns, property) {
        const deprecatedTypeFqn = this.tryFindReference(property.type, strippedFqns);
        if (deprecatedTypeFqn) {
            return [this.makeDiagnostic(deprecatedTypeFqn, 'Property', property, assembly)];
        }
        return [];
    }
    /**
     * Determines whether a `TypeReference` contains an FQN within a given set.
     *
     * @param ref  the tested `TypeReference`.
     * @param fqns the set of FQNs that are being searched for.
     *
     * @returns the first FQN that was identified.
     */
    tryFindReference(ref, fqns) {
        if ((0, spec_1.isNamedTypeReference)(ref)) {
            return fqns.has(ref.fqn) ? ref.fqn : undefined;
        }
        if ((0, spec_1.isPrimitiveTypeReference)(ref)) {
            return undefined;
        }
        if ((0, spec_1.isCollectionTypeReference)(ref)) {
            return this.tryFindReference(ref.collection.elementtype, fqns);
        }
        return ref.union.types.map((type) => this.tryFindReference(type, fqns)).find((typeRef) => typeRef != null);
    }
    shouldFqnBeStripped(fqn) {
        return this.allowlistedDeprecations?.has(fqn) ?? true;
    }
    makeDiagnostic(fqn, messagePrefix, context, assembly) {
        const node = bindings.getRelatedNode(context);
        const diagnostic = jsii_diagnostic_1.JsiiDiagnostic.JSII_3999_INCOHERENT_TYPE_MODEL.create(node?.type ?? node, `${messagePrefix} has @deprecated type ${fqn}, and it is erased by --strip-deprecated.`);
        const typeInfo = assembly.types?.[fqn];
        const typeNode = typeInfo && bindings.getTypeRelatedNode(typeInfo);
        if (typeNode == null) {
            return diagnostic;
        }
        return diagnostic.addRelatedInformation(ts.getNameOfDeclaration(typeNode) ?? typeNode, 'The @deprecated type is declared here');
    }
}
exports.DeprecatedRemover = DeprecatedRemover;
class Transformation {
    static addInterface(typeChecker, node, iface) {
        return new Transformation(typeChecker, node, (declaration) => {
            if (!ts.isClassDeclaration(declaration) && !ts.isInterfaceDeclaration(declaration)) {
                throw new utils_1.JsiiError(`Expected a ClassDeclaration or InterfaceDeclaration, found a ${ts.SyntaxKind[declaration.kind]}`);
            }
            const { typeExpression: newInterface, syntheticImport } = Transformation.typeReference(iface, declaration, typeChecker);
            if (ts.isClassDeclaration(declaration)) {
                return {
                    node: ts.factory.updateClassDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, addInterfaceTo(ts.SyntaxKind.ImplementsKeyword, declaration.heritageClauses), declaration.members),
                    syntheticImport,
                };
            }
            return {
                node: ts.factory.updateInterfaceDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, addInterfaceTo(ts.SyntaxKind.ExtendsKeyword, declaration.heritageClauses), declaration.members),
                syntheticImport,
            };
            function addInterfaceTo(token, clauses = []) {
                const existingClause = clauses.find((clause) => clause.token === token);
                if (existingClause == null) {
                    return [...clauses, ts.factory.createHeritageClause(token, [newInterface])];
                }
                return [
                    ...clauses.filter((clause) => clause !== existingClause),
                    ts.factory.updateHeritageClause(existingClause, [...existingClause.types, newInterface]),
                ];
            }
        });
    }
    static replaceBaseClass(typeChecker, node, baseClass) {
        return new Transformation(typeChecker, node, (declaration) => {
            if (!ts.isClassDeclaration(declaration)) {
                throw new utils_1.JsiiError(`Expected a ClassDeclaration, found a ${ts.SyntaxKind[declaration.kind]}`);
            }
            const { typeExpression: newBaseClass, syntheticImport } = Transformation.typeReference(baseClass, declaration, typeChecker);
            const existingClause = declaration.heritageClauses?.find((clause) => clause.token === ts.SyntaxKind.ExtendsKeyword);
            return {
                node: ts.factory.updateClassDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, [
                    ...(declaration.heritageClauses ?? []).filter((clause) => clause !== existingClause),
                    existingClause
                        ? ts.factory.updateHeritageClause(existingClause, [newBaseClass])
                        : ts.factory.createHeritageClause(ts.SyntaxKind.ExtendsKeyword, [newBaseClass]),
                ], declaration.members),
                syntheticImports: syntheticImport && [syntheticImport],
            };
        });
    }
    static removeBaseClass(typeChecker, node) {
        return new Transformation(typeChecker, node, (declaration) => {
            if (!ts.isClassDeclaration(declaration)) {
                throw new utils_1.JsiiError(`Expected a ClassDeclaration, found a ${ts.SyntaxKind[declaration.kind]}`);
            }
            return {
                node: ts.factory.updateClassDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, declaration.heritageClauses?.filter((clause) => clause.token !== ts.SyntaxKind.ExtendsKeyword), declaration.members),
            };
        });
    }
    static removeInterface(typeChecker, node, iface) {
        const ifaceName = Transformation.fullyQualifiedName(typeChecker, iface);
        return new Transformation(typeChecker, node, (declaration) => {
            if (ts.isClassDeclaration(declaration)) {
                return {
                    node: ts.factory.updateClassDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, removeInterfaceHeritage(declaration.heritageClauses), declaration.members),
                };
            }
            else if (ts.isInterfaceDeclaration(declaration)) {
                return {
                    node: ts.factory.updateInterfaceDeclaration(declaration, declaration.modifiers, declaration.name, declaration.typeParameters, removeInterfaceHeritage(declaration.heritageClauses), declaration.members),
                };
            }
            throw new utils_1.JsiiError(`Expected a ClassDeclaration or InterfaceDeclaration, found a ${ts.SyntaxKind[declaration.kind]}`);
        });
        function removeInterfaceHeritage(clauses) {
            if (clauses == null) {
                return clauses;
            }
            return clauses
                .map((clause) => {
                const types = clause.types.filter((type) => Transformation.fullyQualifiedName(typeChecker, type.expression) !== ifaceName);
                if (types.length === clause.types.length) {
                    // Means the interface was only transitively present...
                    return clause;
                }
                if (types.length === 0) {
                    return undefined;
                }
                return ts.factory.updateHeritageClause(clause, types);
            })
                .filter((clause) => clause != null);
        }
    }
    static fullyQualifiedName(typeChecker, node) {
        const symbol = typeChecker.getSymbolAtLocation(ts.getNameOfDeclaration(node) ?? node);
        // This symbol ☝️ does not contain enough information in some cases - when
        // an imported type is part of a heritage clause - to produce the fqn.
        // Round tripping this to its type and back to a symbol seems to fix this.
        const type = symbol && typeChecker.getDeclaredTypeOfSymbol(symbol);
        return type?.symbol && typeChecker.getFullyQualifiedName(type.symbol);
    }
    static typeReference(type, context, typeChecker) {
        context = ts.getOriginalNode(context);
        const [, contextSource] = /^"([^"]+)"\..*$/.exec(typeChecker.getFullyQualifiedName(typeChecker.getSymbolAtLocation(ts.getNameOfDeclaration(context))));
        let expression;
        let syntheticImport;
        if (typeof type === 'string') {
            const [root, ...tail] = type.split('.');
            const syntheticImportName = ts.factory.createUniqueName(root);
            syntheticImport = ts.factory.createImportDeclaration(undefined /* decorators */, ts.factory.createImportClause(false, undefined, ts.factory.createNamespaceImport(syntheticImportName)), ts.factory.createStringLiteral(root));
            expression = tail.reduce((curr, elt) => ts.factory.createPropertyAccessExpression(curr, elt), syntheticImportName);
        }
        else {
            const [, typeSource, qualifiedName] = /^"([^"]+)"\.(.*)$/.exec(typeChecker.getFullyQualifiedName(typeChecker.getSymbolAtLocation(ts.getNameOfDeclaration(type))));
            if (typeSource === contextSource) {
                const [root, ...tail] = qualifiedName.split('.');
                expression = tail.reduce((curr, elt) => ts.factory.createPropertyAccessExpression(curr, elt), ts.factory.createIdentifier(root));
            }
            else {
                const syntheticImportName = ts.factory.createUniqueName((0, node_path_1.basename)(typeSource));
                syntheticImport = ts.factory.createImportDeclaration(undefined /* modifiers */, ts.factory.createImportClause(false, undefined, ts.factory.createNamespaceImport(syntheticImportName)), ts.factory.createStringLiteral(`./${(0, node_path_1.relative)((0, node_path_1.dirname)(contextSource), typeSource)}`), undefined);
                expression = qualifiedName
                    .split('.')
                    .reduce((curr, elt) => ts.factory.createPropertyAccessExpression(curr, elt), syntheticImportName);
            }
        }
        return {
            typeExpression: ts.factory.createExpressionWithTypeArguments(expression, undefined),
            syntheticImport,
        };
    }
    constructor(typeChecker, node, apply) {
        this.typeChecker = typeChecker;
        this.apply = apply;
        this.nodeName = Transformation.fullyQualifiedName(typeChecker, node);
    }
    targets(node) {
        return this.nodeName === Transformation.fullyQualifiedName(this.typeChecker, node);
    }
}
class DeprecationRemovalTransformer {
    constructor(typeChecker, context, transformations, nodesToRemove) {
        this.typeChecker = typeChecker;
        this.context = context;
        this.transformations = transformations;
        this.nodesToRemove = nodesToRemove;
        this.syntheticImports = new Array();
    }
    transform(node) {
        let result = this.visitEachChild(node);
        // If there are any synthetic imports, add them to the source file
        if (ts.isSourceFile(result) && this.syntheticImports.length > 0) {
            result = this.context.factory.updateSourceFile(result, [...this.syntheticImports, ...result.statements], result.isDeclarationFile, result.referencedFiles, result.typeReferenceDirectives, result.hasNoDefaultLib, result.libReferenceDirectives);
            this.syntheticImports = new Array();
        }
        return result;
    }
    visitEachChild(node) {
        return ts.visitEachChild(node, this.visitor.bind(this), this.context);
    }
    visitor(node) {
        if (this.isDeprecated(node)) {
            // Removing deprecated members by substituting "nothing" to them
            return [];
        }
        if (ts.isClassDeclaration(node) || ts.isInterfaceDeclaration(node)) {
            for (const transformation of this.transformations) {
                // 👇 as any because the assignment below confuses type checker
                if (transformation.targets(node)) {
                    const { node: transformedNode, syntheticImport } = transformation.apply(node);
                    node = transformedNode;
                    if (syntheticImport) {
                        this.syntheticImports.push(syntheticImport);
                    }
                }
            }
        }
        // Remove named imports of `@deprecated` members from the source...
        if (ts.isImportDeclaration(node) &&
            node.importClause &&
            node.importClause.namedBindings &&
            ts.isNamedImports(node.importClause.namedBindings)) {
            const filteredElements = node.importClause.namedBindings.elements.filter((element) => {
                // This symbol is local (it's declaration points back to the named import)
                const symbol = this.typeChecker.getSymbolAtLocation(element.name);
                const exportedSymbol = 
                // This "resolves" the imported type, so we can get to it's declaration(s)
                symbol && this.typeChecker.getDeclaredTypeOfSymbol(symbol)?.symbol;
                return !exportedSymbol?.declarations?.some((decl) => this.isDeprecated(decl));
            });
            if (filteredElements.length !== node.importClause.namedBindings.elements.length) {
                return this.context.factory.updateImportDeclaration(node, node.modifiers, node.importClause.name != null || filteredElements.length > 0
                    ? this.context.factory.updateImportClause(node.importClause, node.importClause.isTypeOnly, node.importClause.name, this.context.factory.updateNamedImports(node.importClause.namedBindings, filteredElements))
                    : undefined, node.moduleSpecifier, node.assertClause);
            }
            return node;
        }
        // Replace "export ... from ..." places that no longer export anything
        // with an "import from ...", so side effects are preserved.
        if (ts.isExportDeclaration(node) && node.moduleSpecifier) {
            const symbol = this.typeChecker.getSymbolAtLocation(node.moduleSpecifier);
            const moduleExports = symbol &&
                this.typeChecker
                    .getExportsOfModule(symbol)
                    ?.filter((sym) => !sym.declarations?.some((decl) => this.isDeprecated(decl)));
            if ((node.exportClause == null || ts.isNamespaceExport(node.exportClause)) && moduleExports?.length === 0) {
                return this.context.factory.createImportDeclaration(undefined /* modifiers */, undefined /* importClause */, node.moduleSpecifier);
            }
            if (node.exportClause != null && moduleExports) {
                const namedExports = node.exportClause;
                const exportedNames = new Set(moduleExports.map((sym) => sym.name));
                const filteredElements = namedExports.elements?.filter((elt) => exportedNames.has(elt.name.text));
                if (filteredElements?.length !== namedExports.elements?.length) {
                    return this.context.factory.updateExportDeclaration(node, node.modifiers, node.isTypeOnly, this.context.factory.updateNamedExports(namedExports, filteredElements), node.moduleSpecifier, node.assertClause);
                }
            }
        }
        return DeprecationRemovalTransformer.IGNORE_CHILDREN.has(node.kind) ? node : this.visitEachChild(node);
    }
    isDeprecated(node) {
        const original = ts.getOriginalNode(node);
        return (this.nodesToRemove.has(original) && ts.getJSDocTags(original).some((tag) => tag.tagName.text === 'deprecated'));
    }
}
/**
 * A list of SyntaxKinds for which it is not necessary to evaluate children,
 * since they are never of interest to this transform. This opens up a wee
 * optimization, which is particularly useful when trying to troubleshoot the
 * transform in a debugger (saves a TON of time stepping into useless nodes
 * then).
 */
DeprecationRemovalTransformer.IGNORE_CHILDREN = new Set([
    ts.SyntaxKind.Constructor,
    ts.SyntaxKind.FunctionDeclaration,
    ts.SyntaxKind.GetAccessor,
    ts.SyntaxKind.MethodDeclaration,
    ts.SyntaxKind.MethodSignature,
    ts.SyntaxKind.PropertySignature,
    ts.SyntaxKind.PropertyDeclaration,
    ts.SyntaxKind.SetAccessor,
    ts.SyntaxKind.VariableDeclaration,
]);
//# sourceMappingURL=deprecated-remover.js.map