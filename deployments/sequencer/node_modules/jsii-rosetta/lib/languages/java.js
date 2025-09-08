"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JavaVisitor = void 0;
const ts = require("typescript");
const default_1 = require("./default");
const jsii_types_1 = require("../jsii/jsii-types");
const jsii_utils_1 = require("../jsii/jsii-utils");
const packages_1 = require("../jsii/packages");
const target_language_1 = require("../languages/target-language");
const o_tree_1 = require("../o-tree");
const ast_utils_1 = require("../typescript/ast-utils");
const types_1 = require("../typescript/types");
const util_1 = require("../util");
class JavaVisitor extends default_1.DefaultVisitor {
    constructor() {
        super(...arguments);
        /**
         * Aliases for modules
         *
         * If these are encountered in the LHS of a property access, they will be dropped.
         */
        this.dropPropertyAccesses = new Set();
        this.language = target_language_1.TargetLanguage.JAVA;
        this.defaultContext = {};
    }
    mergeContext(old, update) {
        return Object.assign({}, old, update);
    }
    importStatement(importStatement) {
        const guessedNamespace = guessJavaNamespaceName(importStatement.packageName);
        if (importStatement.imports.import === 'full') {
            this.dropPropertyAccesses.add(importStatement.imports.sourceName);
            const namespace = (0, util_1.fmap)(importStatement.moduleSymbol, findJavaName) ?? guessedNamespace;
            return new o_tree_1.OTree([`import ${namespace}.*;`], [], { canBreakLine: true });
        }
        const imports = importStatement.imports.elements.map((e) => {
            const fqn = (0, util_1.fmap)(e.importedSymbol, findJavaName) ?? `${guessedNamespace}.${e.sourceName}`;
            // If there is no imported symbol, we check if there is anything looking like a type name in
            // the source name (that is, any segment that starts with an upper case letter), and if none
            // is found, assume this refers to a namespace/module.
            return (e.importedSymbol?.symbolType == null &&
                !e.sourceName.split('.').some((segment) => /^[A-Z]/.test(segment))) ||
                e.importedSymbol?.symbolType === 'module'
                ? `import ${fqn}.*;`
                : `import ${fqn};`;
        });
        const localNames = importStatement.imports.elements
            .filter((el) => el.importedSymbol?.symbolType === 'module')
            .map((el) => el.alias ?? el.sourceName);
        (0, util_1.setExtend)(this.dropPropertyAccesses, localNames);
        return new o_tree_1.OTree([], imports, { canBreakLine: true, separator: '\n' });
    }
    classDeclaration(node, renderer) {
        return this.renderClassDeclaration(node, renderer);
    }
    structInterfaceDeclaration(node, renderer) {
        // Render structs as simple Java classes with getters, and setters that return `this`.
        // This is a compromise between brevity
        // (rendering a full inner static Builder class, like JSII uses, would be quite verbose)
        // and ease of use
        // (fluent setters allow us to mirror JavaScript object literals more closely than classic,
        // void-returning setters would).
        return this.renderClassDeclaration(node, renderer);
    }
    regularInterfaceDeclaration(node, renderer) {
        return new o_tree_1.OTree([
            'public ',
            'interface ',
            renderer.convert(node.name),
            ...this.typeHeritage(node, renderer.updateContext({ discardPropertyAccess: true })),
            ' {',
        ], renderer
            .updateContext({
            insideTypeDeclaration: { typeName: node.name, isInterface: true },
        })
            .convertAll(node.members), {
            indent: 4,
            canBreakLine: true,
            suffix: '\n}',
        });
    }
    propertySignature(node, renderer) {
        const propertyType = this.renderTypeNode(node.type, renderer, 'Object');
        const propertyName = renderer.convert(node.name);
        const isClass = !renderer.currentContext.insideTypeDeclaration?.isInterface;
        const blockSep = isClass ? ' ' : ';';
        const field = isClass
            ? new o_tree_1.OTree([], ['private ', propertyType, ' ', propertyName, ';'], {
                canBreakLine: true,
            })
            : o_tree_1.NO_SYNTAX;
        const getter = new o_tree_1.OTree([], [
            isClass ? 'public ' : o_tree_1.NO_SYNTAX,
            propertyType,
            ' ',
            `get${capitalize(renderer.textOf(node.name))}()${blockSep}`,
            isClass ? this.renderBlock([new o_tree_1.OTree(['\n'], ['return this.', propertyName, ';'])]) : o_tree_1.NO_SYNTAX,
        ], {
            canBreakLine: true,
        });
        const hasSetter = isClass || !(0, ast_utils_1.isReadOnly)(node);
        const setter = hasSetter
            ? new o_tree_1.OTree([], [
                isClass ? 'public ' : o_tree_1.NO_SYNTAX,
                renderer.convert(renderer.currentContext.insideTypeDeclaration.typeName),
                ' ',
                propertyName, // don't prefix the setter with `set` - makes it more aligned with JSII builders
                '(',
                propertyType,
                ' ',
                propertyName,
                `)${blockSep}`,
                isClass
                    ? this.renderBlock([
                        new o_tree_1.OTree(['\n'], ['this.', propertyName, ' = ', propertyName, ';']),
                        new o_tree_1.OTree(['\n'], ['return this;']),
                    ])
                    : o_tree_1.NO_SYNTAX,
            ], {
                canBreakLine: true,
            })
            : o_tree_1.NO_SYNTAX;
        return new o_tree_1.OTree([], [field, getter, setter], {
            canBreakLine: true,
            separator: '\n',
        });
    }
    propertyDeclaration(node, renderer) {
        const vis = (0, ast_utils_1.visibility)(node);
        return new o_tree_1.OTree([
            vis,
            (0, ast_utils_1.isReadOnly)(node) ? ' final' : '',
            ' ',
            this.renderTypeNode(node.type, renderer, 'Object'),
            ' ',
            renderer.convert(node.name),
            ';',
        ], [], {
            canBreakLine: true,
        });
    }
    constructorDeclaration(node, renderer) {
        return this.renderProcedure(node, renderer, renderer.currentContext.insideTypeDeclaration.typeName, undefined);
    }
    methodDeclaration(node, renderer) {
        const retType = (0, types_1.determineReturnType)(renderer.typeChecker, node);
        return this.renderProcedure(node, renderer, node.name, this.renderType(node, retType, renderer, 'void'));
    }
    functionDeclaration(node, renderer) {
        const retType = (0, types_1.determineReturnType)(renderer.typeChecker, node);
        return this.renderProcedure(node, renderer, node.name, this.renderType(node, retType, renderer, 'void'));
    }
    methodSignature(node, renderer) {
        return new o_tree_1.OTree([
            this.renderTypeNode(node.type, renderer, 'void'),
            ' ',
            renderer.convert(node.name),
            '(',
            new o_tree_1.OTree([], renderer.convertAll(node.parameters), {
                separator: ', ',
            }),
            ');',
        ], [], { canBreakLine: true });
    }
    parameterDeclaration(node, renderer) {
        let renderedType = this.renderTypeNode(node.type, renderer);
        if (node.dotDotDotToken && renderedType.endsWith('[]')) {
            // Varargs. In Java, render this as `Element...` (instead of `Element[]` which is what we'll have gotten).
            renderedType = `${renderedType.slice(0, -2)}...`;
        }
        return new o_tree_1.OTree([renderedType, ' ', renderer.convert(node.name)]);
    }
    block(node, renderer) {
        return this.renderBlock(renderer.convertAll(node.statements));
    }
    variableDeclaration(node, renderer) {
        let fallback = 'Object';
        if (node.type) {
            fallback = node.type.getText();
        }
        const type = (node.type && renderer.typeOfType(node.type)) ||
            (node.initializer && renderer.typeOfExpression(node.initializer));
        const renderedType = type ? this.renderType(node, type, renderer, fallback) : fallback;
        if (!node.initializer) {
            return new o_tree_1.OTree([renderedType, ' ', renderer.convert(node.name), ';'], []);
        }
        return new o_tree_1.OTree([
            renderedType,
            ' ',
            renderer.convert(node.name),
            ...(node.initializer ? [' = ', renderer.convert(node.initializer)] : []),
            ';',
        ], [], {
            canBreakLine: true,
        });
    }
    expressionStatement(node, renderer) {
        const inner = renderer.convert(node.expression);
        return inner.isEmpty ? inner : new o_tree_1.OTree([inner, ';'], [], { canBreakLine: true });
    }
    ifStatement(node, renderer) {
        const ifStmt = new o_tree_1.OTree(['if (', renderer.convert(node.expression), ') '], [renderer.convert(node.thenStatement)], {
            canBreakLine: true,
        });
        const elseStmt = node.elseStatement
            ? new o_tree_1.OTree(['else '], [renderer.convert(node.elseStatement)], {
                canBreakLine: true,
            })
            : undefined;
        return elseStmt
            ? new o_tree_1.OTree([], [ifStmt, elseStmt], {
                separator: ' ',
                canBreakLine: true,
            })
            : ifStmt;
    }
    forOfStatement(node, renderer) {
        // This is what a "for (const x of ...)" looks like in the AST
        let variableName = '???';
        (0, ast_utils_1.matchAst)(node.initializer, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.VariableDeclarationList, (0, ast_utils_1.nodeOfType)('var', ts.SyntaxKind.VariableDeclaration)), (bindings) => {
            variableName = renderer.textOf(bindings.var.name);
        });
        return new o_tree_1.OTree(['for (Object ', variableName, ' : ', renderer.convert(node.expression), ') '], [renderer.convert(node.statement)], {
            canBreakLine: true,
        });
    }
    printStatement(args, renderer) {
        return new o_tree_1.OTree([
            'System.out.println(',
            args.length === 1 ? renderer.convert(args[0]) : new o_tree_1.OTree([], renderer.convertAll(args), { separator: ' + ' }),
            ')',
        ]);
    }
    templateExpression(node, renderer) {
        let template = '';
        const parameters = new Array();
        if (node.head.rawText) {
            template += node.head.rawText;
        }
        for (const span of node.templateSpans) {
            template += '%s';
            parameters.push(renderer
                .updateContext({
                convertPropertyToGetter: true,
                identifierAsString: false,
            })
                .convert(span.expression));
            if (span.literal.rawText) {
                template += span.literal.rawText;
            }
        }
        if (parameters.length === 0) {
            return new o_tree_1.OTree([JSON.stringify((0, ast_utils_1.quoteStringLiteral)(template))]);
        }
        return new o_tree_1.OTree([
            'String.format(',
            `"${(0, ast_utils_1.quoteStringLiteral)(template)
                // Java does not have multiline string literals, so we must replace literal newlines with %n
                .replace(/\n/g, '%n')}"`,
            ...parameters.reduce((list, element) => list.concat(', ', element), new Array()),
            ')',
        ]);
    }
    asExpression(node, renderer) {
        return new o_tree_1.OTree(['(', this.renderTypeNode(node.type, renderer, 'Object'), ')', renderer.convert(node.expression)]);
    }
    arrayLiteralExpression(node, renderer) {
        return new o_tree_1.OTree(['List.of('], renderer.convertAll(node.elements), {
            separator: ', ',
            suffix: ')',
            indent: 4,
        });
    }
    regularCallExpression(node, renderer) {
        return new o_tree_1.OTree([
            renderer.updateContext({ convertPropertyToGetter: false }).convert(node.expression),
            '(',
            this.argumentList(node.arguments, renderer),
            ')',
        ]);
    }
    newExpression(node, renderer) {
        const argsLength = node.arguments ? node.arguments.length : 0;
        const lastArg = argsLength > 0 ? node.arguments[argsLength - 1] : undefined;
        // We render the ClassName.Builder.create(...) expression
        // if the last argument is an object literal, and either a known struct (because
        // those are the jsii rules) or an unknown type (because the example didn't
        // compile but most of the target examples will intend this to be a struct).
        const structArgument = lastArg && ts.isObjectLiteralExpression(lastArg) ? lastArg : undefined;
        let renderBuilder = false;
        if (lastArg && ts.isObjectLiteralExpression(lastArg)) {
            const analysis = (0, jsii_types_1.analyzeObjectLiteral)(renderer.typeChecker, lastArg);
            renderBuilder = analysis.kind === 'struct' || analysis.kind === 'unknown';
        }
        const className = renderer
            .updateContext({
            discardPropertyAccess: true,
            convertPropertyToGetter: false,
        })
            .convert(node.expression);
        if (renderBuilder) {
            const initialArguments = node.arguments.slice(0, argsLength - 1);
            return new o_tree_1.OTree([], [
                className,
                '.Builder.create(',
                this.argumentList(initialArguments, renderer),
                ')',
                ...renderer.convertAll(structArgument.properties),
                new o_tree_1.OTree([renderer.mirrorNewlineBefore(structArgument.properties[0])], ['.build()']),
            ], { canBreakLine: true, indent: 8 });
        }
        return new o_tree_1.OTree([], ['new ', className, '(', this.argumentList(node.arguments, renderer), ')'], {
            canBreakLine: true,
        });
    }
    unknownTypeObjectLiteralExpression(node, renderer) {
        return renderer.currentContext.inNewExprWithObjectLiteralAsLastArg
            ? this.renderObjectLiteralAsBuilder(node, renderer)
            : this.keyValueObjectLiteralExpression(node, renderer);
    }
    keyValueObjectLiteralExpression(node, renderer) {
        return new o_tree_1.OTree(['Map.of('], renderer.updateContext({ inKeyValueList: true }).convertAll(node.properties), {
            suffix: ')',
            separator: ', ',
            indent: 8,
        });
    }
    knownStructObjectLiteralExpression(node, structType, renderer) {
        // Special case: we're rendering an object literal, but the containing constructor
        // has already started the builder: we don't have to create a new instance,
        // all we have to do is pile on arguments.
        if (renderer.currentContext.inNewExprWithObjectLiteralAsLastArg) {
            return new o_tree_1.OTree([], renderer.convertAll(node.properties), { indent: 8 });
        }
        // Jsii-generated classes have builders, classes we generated in the course of
        // this example do not.
        const hasBuilder = structType.kind !== 'local-struct';
        return new o_tree_1.OTree(hasBuilder ? [structType.type.symbol.name, '.builder()'] : ['new ', structType.type.symbol.name, '()'], [
            ...renderer.convertAll(node.properties),
            new o_tree_1.OTree([renderer.mirrorNewlineBefore(node.properties[0])], [hasBuilder ? '.build()' : '']),
        ], {
            indent: 8,
        });
    }
    propertyAssignment(node, renderer) {
        return renderer.currentContext.inKeyValueList
            ? this.singlePropertyInJavaScriptObjectLiteralToJavaMap(node.name, node.initializer, renderer)
            : this.singlePropertyInJavaScriptObjectLiteralToFluentSetters(node.name, node.initializer, renderer);
    }
    shorthandPropertyAssignment(node, renderer) {
        return renderer.currentContext.inKeyValueList
            ? this.singlePropertyInJavaScriptObjectLiteralToJavaMap(node.name, node.name, renderer)
            : this.singlePropertyInJavaScriptObjectLiteralToFluentSetters(node.name, node.name, renderer);
    }
    propertyAccessExpression(node, renderer, submoduleRef) {
        const rightHandSide = renderer.convert(node.name);
        // If a submodule access, then just render the name, we emitted a * import of the expression segment already.
        if (submoduleRef != null) {
            return rightHandSide;
        }
        let parts;
        const leftHandSide = renderer.textOf(node.expression);
        // Suppress the LHS of the dot operator if it matches an alias for a module import.
        if (this.dropPropertyAccesses.has(leftHandSide) || renderer.currentContext.discardPropertyAccess) {
            parts = [rightHandSide];
        }
        else if (leftHandSide === 'this') {
            // for 'this', assume this is a field, and access it directly
            parts = ['this', '.', rightHandSide];
        }
        else {
            let convertToGetter = renderer.currentContext.convertPropertyToGetter !== false;
            // See if we're not accessing an enum member or public static readonly property (const).
            if ((0, types_1.isEnumAccess)(renderer.typeChecker, node)) {
                convertToGetter = false;
            }
            if ((0, types_1.isStaticReadonlyAccess)(renderer.typeChecker, node)) {
                convertToGetter = false;
            }
            // add a 'get' prefix to the property name, and change the access to a method call, if required
            const renderedRightHandSide = convertToGetter ? `get${capitalize(node.name.text)}()` : rightHandSide;
            // strip any trailing ! from the left-hand side, as they're not meaningful in Java
            parts = [renderer.convert(node.expression), '.', renderedRightHandSide];
        }
        return new o_tree_1.OTree(parts);
    }
    stringLiteral(node, renderer) {
        if (renderer.currentContext.stringLiteralAsIdentifier) {
            return this.identifier(node, renderer);
        }
        // Java does not have multiline string literals, so we must replace literal newlines with \n
        return new o_tree_1.OTree([JSON.stringify(node.text).replace(/\n/g, '\\n')]);
    }
    identifier(node, renderer) {
        const nodeText = node.text;
        return new o_tree_1.OTree([renderer.currentContext.identifierAsString ? JSON.stringify(nodeText) : nodeText]);
    }
    renderObjectLiteralAsBuilder(node, renderer) {
        return new o_tree_1.OTree([], [
            ...renderer.convertAll(node.properties),
            new o_tree_1.OTree([renderer.mirrorNewlineBefore(node.properties[0])], ['.build()']),
        ], {
            indent: 8,
        });
    }
    singlePropertyInJavaScriptObjectLiteralToJavaMap(name, initializer, renderer) {
        return new o_tree_1.OTree([], [
            renderer
                .updateContext({
                identifierAsString: !ts.isComputedPropertyName(name),
            })
                .convert(name),
            ', ',
            renderer.updateContext({ inKeyValueList: false }).convert(initializer),
        ], {
            canBreakLine: true,
        });
    }
    singlePropertyInJavaScriptObjectLiteralToFluentSetters(name, initializer, renderer) {
        return new o_tree_1.OTree([], [
            '.',
            renderer.updateContext({ stringLiteralAsIdentifier: true }).convert(name),
            '(',
            renderer.updateContext({ inNewExprWithObjectLiteralAsLastArg: false }).convert(initializer),
            ')',
        ], {
            canBreakLine: true,
        });
    }
    renderClassDeclaration(node, renderer) {
        return new o_tree_1.OTree([
            'public ',
            'class ',
            renderer.convert(node.name),
            ...this.typeHeritage(node, renderer.updateContext({ discardPropertyAccess: true })),
            ' {',
        ], renderer.updateContext({ insideTypeDeclaration: { typeName: node.name } }).convertAll(node.members), {
            indent: 4,
            canBreakLine: true,
            suffix: '\n}',
        });
    }
    typeHeritage(node, renderer) {
        return [
            ...this.extractSuperTypes(node, renderer, ts.SyntaxKind.ExtendsKeyword, 'extends'),
            ...this.extractSuperTypes(node, renderer, ts.SyntaxKind.ImplementsKeyword, 'implements'),
        ];
    }
    extractSuperTypes(node, renderer, heritageKeyword, outputKeyword) {
        const heritageClause = (node.heritageClauses ?? []).find((hc) => hc.token === heritageKeyword);
        const superTypes = heritageClause ? heritageClause.types.map((t) => renderer.convert(t.expression)) : [];
        return superTypes.length > 0 ? [` ${outputKeyword} `, new o_tree_1.OTree([], superTypes, { separator: ', ' })] : [];
    }
    renderTypeNode(typeNode, renderer, fallback) {
        fallback =
            fallback ??
                (typeNode
                    ? lastElement(renderer.textOf(typeNode).split('.')) // remove any namespace prefixes
                    : 'Object');
        if (!typeNode) {
            return fallback;
        }
        return this.renderType(typeNode, renderer.typeOfType(typeNode), renderer, fallback);
    }
    renderType(owningNode, type, renderer, fallback) {
        if (!type) {
            return fallback;
        }
        return doRender((0, jsii_types_1.determineJsiiType)(renderer.typeChecker, type), false);
        // eslint-disable-next-line consistent-return
        function doRender(jsiiType, requiresReferenceType) {
            switch (jsiiType.kind) {
                case 'unknown':
                    return fallback;
                case 'error':
                    renderer.report(owningNode, jsiiType.message);
                    return fallback;
                case 'map':
                    return `Map<String, ${doRender(jsiiType.elementType, true)}>`;
                case 'list':
                    return `${doRender(jsiiType.elementType, true)}[]`;
                case 'namedType':
                    return jsiiType.name;
                case 'builtIn':
                    switch (jsiiType.builtIn) {
                        case 'boolean':
                            return requiresReferenceType ? 'Boolean' : 'boolean';
                        case 'number':
                            return 'Number';
                        case 'string':
                            return 'String';
                        case 'any':
                            return 'Object';
                        default:
                            return jsiiType.builtIn;
                    }
            }
        }
    }
    renderProcedure(node, renderer, methodOrConstructorName, returnType) {
        const overloads = new Array();
        for (let i = 0; i < node.parameters.length; i++) {
            const param = node.parameters[i];
            if (!!param.questionToken || !!param.initializer) {
                // The parameter is either optional, or has a default -
                // render an overload that delegates to a version with one more parameter.
                // Note that we don't check that all parameters with indexes > i are also optional/have a default -
                // we assume the TypeScript compiler does that for us.
                // parameters up to but excluding the current one
                const parametersUpToIth = node.parameters.slice(0, i);
                // how should the call to the next overload look
                const callExpr = ts.isConstructorDeclaration(node) ? 'this' : renderer.convert(methodOrConstructorName);
                overloads.push(this.renderOverload(returnType, renderer, methodOrConstructorName, parametersUpToIth, 
                // the body is the call to the next overload
                this.renderBlock([
                    new o_tree_1.OTree(['\n', callExpr, '('], [
                        ...parametersUpToIth.map((p) => renderer.convert(p.name)),
                        param.initializer ? renderer.convert(param.initializer) : 'null',
                    ], {
                        separator: ', ',
                        suffix: ');',
                    }),
                ])));
            }
        }
        // render the primary overload
        overloads.push(this.renderOverload(returnType, renderer, methodOrConstructorName, node.parameters, renderer.convert(node.body)));
        return new o_tree_1.OTree([], overloads, {
            canBreakLine: true,
            separator: '\n\n',
        });
    }
    renderOverload(returnType, renderer, methodOrConstructorName, parameters, body) {
        return new o_tree_1.OTree([
            'public ',
            returnType ? `${returnType} ` : undefined,
            renderer.convert(methodOrConstructorName),
            '(',
            new o_tree_1.OTree([], renderer.convertAll(parameters), { separator: ', ' }),
            ') ',
        ], [body], {
            canBreakLine: true,
        });
    }
    renderBlock(blockContents) {
        return new o_tree_1.OTree(['{'], blockContents, {
            indent: 4,
            suffix: '\n}',
        });
    }
}
exports.JavaVisitor = JavaVisitor;
/**
 * Translation version
 *
 * Bump this when you change something in the implementation to invalidate
 * existing cached translations.
 */
JavaVisitor.VERSION = '1';
function capitalize(str) {
    return str.charAt(0).toUpperCase() + str.slice(1);
}
function lastElement(strings) {
    return strings[strings.length - 1];
}
/**
 * Find the Java name of a module or type
 */
function findJavaName(jsiiSymbol) {
    if (!jsiiSymbol.sourceAssembly?.assembly) {
        // Don't have accurate info, just guess
        return jsiiSymbol.symbolType !== 'module' ? (0, jsii_utils_1.simpleName)(jsiiSymbol.fqn) : guessJavaNamespaceName(jsiiSymbol.fqn);
    }
    const asm = jsiiSymbol.sourceAssembly?.assembly;
    return recurse(jsiiSymbol.fqn);
    function recurse(fqn) {
        if (fqn === asm.name) {
            return (0, packages_1.jsiiTargetParameter)(asm, 'java.package') ?? guessJavaNamespaceName(fqn);
        }
        if (asm.submodules?.[fqn]) {
            const modName = (0, packages_1.jsiiTargetParameter)(asm.submodules[fqn], 'java.package');
            if (modName) {
                return modName;
            }
        }
        const ns = (0, jsii_utils_1.namespaceName)(fqn);
        const nsJavaName = recurse(ns);
        const leaf = (0, jsii_utils_1.simpleName)(fqn);
        return `${nsJavaName}.${leaf}`;
    }
}
function guessJavaNamespaceName(packageName) {
    return packageName
        .split(/[^a-zA-Z0-9]+/g)
        .filter((s) => s !== '')
        .join('.');
}
//# sourceMappingURL=java.js.map