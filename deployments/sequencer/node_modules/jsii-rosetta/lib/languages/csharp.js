"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.CSharpVisitor = void 0;
const ts = require("typescript");
const default_1 = require("./default");
const target_language_1 = require("./target-language");
const jsii_types_1 = require("../jsii/jsii-types");
const jsii_utils_1 = require("../jsii/jsii-utils");
const packages_1 = require("../jsii/packages");
const o_tree_1 = require("../o-tree");
const renderer_1 = require("../renderer");
const ast_utils_1 = require("../typescript/ast-utils");
const types_1 = require("../typescript/types");
const util_1 = require("../util");
class CSharpVisitor extends default_1.DefaultVisitor {
    constructor() {
        super(...arguments);
        this.language = target_language_1.TargetLanguage.CSHARP;
        this.defaultContext = {
            propertyOrMethod: false,
            inStructInterface: false,
            inRegularInterface: false,
            inKeyValueList: false,
            stringAsIdentifier: false,
            identifierAsString: false,
            preferObjectLiteralAsStruct: true,
            privatePropertyNames: [],
        };
        /**
         * Aliases for modules
         *
         * If these are encountered in the LHS of a property access, they will be dropped.
         */
        this.dropPropertyAccesses = new Set();
        /**
         * Already imported modules so we don't emit duplicate imports
         */
        this.alreadyImportedNamespaces = new Set();
        /**
         * A map to undo import renames
         *
         * We will always reference the original name in the translation.
         *
         * Maps a local-name to a C# name.
         */
        this.renamedSymbols = new Map();
    }
    mergeContext(old, update) {
        return Object.assign({}, old, update);
    }
    identifier(node, renderer) {
        let text = node.text;
        if (renderer.currentContext.identifierAsString) {
            return new o_tree_1.OTree([JSON.stringify(text)]);
        }
        // Uppercase methods and properties, leave the rest as-is
        if (renderer.currentContext.propertyOrMethod && !renderer.currentContext.privatePropertyNames.includes(text)) {
            text = ucFirst(text);
        }
        return new o_tree_1.OTree([text]);
    }
    importStatement(importStatement, context) {
        const guessedNamespace = guessDotnetNamespace(importStatement.packageName);
        const namespace = (0, util_1.fmap)(importStatement.moduleSymbol, findDotnetName) ?? guessedNamespace;
        if (importStatement.imports.import === 'full') {
            this.dropPropertyAccesses.add(importStatement.imports.sourceName);
            this.alreadyImportedNamespaces.add(namespace);
            return new o_tree_1.OTree([`using ${namespace};`], [], { canBreakLine: true });
        }
        if (importStatement.imports.import === 'selective') {
            const statements = new Array();
            for (const el of importStatement.imports.elements) {
                const dotnetNs = (0, util_1.fmap)(el.importedSymbol, findDotnetName) ?? `${guessedNamespace}.${ucFirst(el.sourceName)}`;
                // If this is an alias, we only honor it if it's NOT for sure a module
                // (could be an alias import of a class or enum).
                if (el.alias && el.importedSymbol?.symbolType !== 'module') {
                    this.renamedSymbols.set(el.alias, (0, jsii_utils_1.simpleName)(dotnetNs));
                    statements.push(`using ${ucFirst(el.alias)} = ${dotnetNs};`);
                    continue;
                }
                // If we are importing a module directly, drop the occurrences of that
                // identifier further down (turn `mod.MyClass` into `MyClass`).
                if (el.importedSymbol?.symbolType === 'module') {
                    this.dropPropertyAccesses.add(el.alias ?? el.sourceName);
                }
                // Output an import statement for the containing namespace
                const importableNamespace = el.importedSymbol?.symbolType === 'module' ? dotnetNs : (0, jsii_utils_1.namespaceName)(dotnetNs);
                if (this.alreadyImportedNamespaces.has(importableNamespace)) {
                    continue;
                }
                this.alreadyImportedNamespaces.add(importableNamespace);
                statements.push(`using ${importableNamespace};`);
            }
            return new o_tree_1.OTree([], statements, { canBreakLine: true, separator: '\n' });
        }
        return (0, renderer_1.nimpl)(importStatement.node, context);
    }
    functionDeclaration(node, renderer) {
        return this.functionLike(node, renderer);
    }
    constructorDeclaration(node, renderer) {
        return this.functionLike(node, renderer, { isConstructor: true });
    }
    methodDeclaration(node, renderer) {
        return this.functionLike(node, renderer);
    }
    methodSignature(node, renderer) {
        return new o_tree_1.OTree([
            this.renderTypeNode(node.type, false, renderer),
            ' ',
            renderer.updateContext({ propertyOrMethod: true }).convert(node.name),
            '(',
            new o_tree_1.OTree([], renderer.convertAll(node.parameters), {
                separator: ', ',
            }),
            ');',
        ], [], { canBreakLine: true });
    }
    // tslint:disable-next-line:max-line-length
    functionLike(node, renderer, opts = {}) {
        const methodName = opts.isConstructor
            ? (0, ast_utils_1.findEnclosingClassDeclaration)(node)?.name?.text ?? 'MyClass'
            : renderer.updateContext({ propertyOrMethod: true }).convert(node.name);
        const retType = (0, types_1.determineReturnType)(renderer.typeChecker, node);
        const returnType = opts.isConstructor ? '' : this.renderType(node, retType, false, 'void', renderer);
        const baseConstructorCall = new Array();
        if (opts.isConstructor) {
            const superCall = (0, ast_utils_1.findSuperCall)(node.body, renderer);
            if (superCall) {
                baseConstructorCall.push(': base(', this.argumentList(superCall.arguments, renderer), ') ');
            }
        }
        const ret = new o_tree_1.OTree([
            (0, ast_utils_1.visibility)(node),
            ' ',
            returnType,
            returnType ? ' ' : '',
            methodName,
            '(',
            new o_tree_1.OTree([], renderer.convertAll(node.parameters), {
                separator: ', ',
            }),
            ') ',
            ...baseConstructorCall,
        ], [renderer.convert(node.body)], {
            canBreakLine: true,
        });
        return ret;
    }
    propertyDeclaration(node, renderer) {
        const vis = (0, ast_utils_1.visibility)(node);
        const propertyOrMethod = vis !== 'private'; // Capitalize non-private fields
        if (vis === 'private' || node.initializer) {
            // Emit member field
            return new o_tree_1.OTree([
                vis,
                (0, ast_utils_1.isReadOnly)(node) ? ' readonly' : '',
                ' ',
                this.renderTypeNode(node.type, node.questionToken !== undefined, renderer),
                ' ',
                renderer.updateContext({ propertyOrMethod }).convert(node.name),
                ...(node.initializer ? [' = ', renderer.convert(node.initializer)] : []),
                ';',
            ], [], { canBreakLine: true });
        }
        // Emit property. No functional difference but slightly more idiomatic
        return new o_tree_1.OTree([
            vis,
            ' ',
            this.renderTypeNode(node.type, node.questionToken !== undefined, renderer),
            ' ',
            renderer.updateContext({ propertyOrMethod }).convert(node.name),
            ' ',
            (0, ast_utils_1.isReadOnly)(node) ? '{ get; }' : '{ get; set; }',
        ], [], { canBreakLine: true });
    }
    printStatement(args, renderer) {
        const renderedArgs = args.length === 1
            ? renderer.convertAll(args)
            : [
                '$"',
                new o_tree_1.OTree([], args.map((a) => new o_tree_1.OTree(['{', renderer.convert(a), '}'])), { separator: ' ' }),
                '"',
            ];
        return new o_tree_1.OTree(['Console.WriteLine(', ...renderedArgs, ')']);
    }
    superCallExpression(_node, _renderer) {
        // super() call rendered as part of the constructor already
        return o_tree_1.NO_SYNTAX;
    }
    stringLiteral(node, renderer) {
        if (renderer.currentContext.stringAsIdentifier) {
            return this.identifier(node, renderer);
        }
        if (node.text.includes('\n')) {
            // Multi-line string literals (@"string") in C# do not do escaping. Only " needs to be doubled.
            return new o_tree_1.OTree(['@"', node.text.replace(/"/g, '""'), '"']);
        }
        return new o_tree_1.OTree([JSON.stringify(node.text)]);
    }
    expressionStatement(node, renderer) {
        const inner = renderer.convert(node.expression);
        if (inner.isEmpty) {
            return inner;
        }
        return new o_tree_1.OTree([inner, ';'], [], { canBreakLine: true });
    }
    propertyAccessExpression(node, renderer) {
        const lhs = renderer.textOf(node.expression);
        // Suppress the LHS of the dot operator if it's "this." (not necessary in C#)
        // or if it's an imported module reference (C# has namespace-wide imports).
        const objectExpression = lhs === 'this' || this.dropPropertyAccesses.has(lhs)
            ? []
            : [renderer.updateContext({ propertyOrMethod: false }).convert(node.expression), '.'];
        return new o_tree_1.OTree([...objectExpression, renderer.updateContext({ propertyOrMethod: true }).convert(node.name)]);
    }
    parameterDeclaration(node, renderer) {
        return new o_tree_1.OTree([
            ...(node.dotDotDotToken ? ['params '] : []), // Varargs. Render with 'params' keyword
            this.renderTypeNode(node.type, node.questionToken !== undefined, renderer),
            ' ',
            renderer.convert(node.name),
            ...((0, types_1.parameterAcceptsUndefined)(node, node.type && renderer.typeOfType(node.type))
                ? ['=', node.initializer ? renderer.convert(node.initializer) : 'null']
                : []),
        ]);
    }
    propertySignature(node, renderer) {
        const canSet = renderer.currentContext.inStructInterface || !(0, ast_utils_1.isReadOnly)(node);
        return new o_tree_1.OTree([
            !renderer.currentContext.inRegularInterface ? `${(0, ast_utils_1.visibility)(node)} ` : o_tree_1.NO_SYNTAX,
            this.renderTypeNode(node.type, node.questionToken !== undefined, renderer),
            ' ',
            renderer.updateContext({ propertyOrMethod: true }).convert(node.name),
            ' ',
            canSet ? '{ get; set; }' : '{ get; }',
        ], [], { canBreakLine: true });
    }
    /**
     * Do some work on property accesses to translate common JavaScript-isms to language-specific idioms
     */
    regularCallExpression(node, renderer) {
        return new o_tree_1.OTree([
            renderer.updateContext({ propertyOrMethod: true }).convert(node.expression),
            '(',
            this.argumentList(node.arguments, renderer),
            ')',
        ]);
    }
    classDeclaration(node, renderer) {
        return new o_tree_1.OTree(['class ', renderer.convert(node.name), ...this.classHeritage(node, renderer), '\n{'], renderer
            .updateContext({
            privatePropertyNames: (0, ast_utils_1.privatePropertyNames)(node.members, renderer),
        })
            .convertAll(node.members), {
            indent: 4,
            canBreakLine: true,
            suffix: '\n}',
        });
    }
    structInterfaceDeclaration(node, renderer) {
        return new o_tree_1.OTree(['class ', renderer.convert(node.name), ...this.classHeritage(node, renderer), '\n{'], renderer.updateContext({ inStructInterface: true }).convertAll(node.members), {
            indent: 4,
            canBreakLine: true,
            suffix: '\n}',
        });
    }
    regularInterfaceDeclaration(node, renderer) {
        return new o_tree_1.OTree(['interface ', renderer.convert(node.name), ...this.classHeritage(node, renderer), '\n{'], renderer.updateContext({ inRegularInterface: true }).convertAll(node.members), {
            indent: 4,
            canBreakLine: true,
            suffix: '\n}',
        });
    }
    block(node, children) {
        return new o_tree_1.OTree(['\n{'], [...children.convertAll(node.statements)], {
            indent: 4,
            suffix: '\n}',
        });
    }
    unknownTypeObjectLiteralExpression(node, renderer) {
        if (renderer.currentContext.preferObjectLiteralAsStruct) {
            // Type information missing and from context we prefer a struct
            return new o_tree_1.OTree(['new Struct { '], renderer.convertAll(node.properties), {
                suffix: renderer.mirrorNewlineBefore(node.properties[0], '}', ' '),
                separator: ', ',
                indent: 4,
            });
        }
        // Type information missing and from context we prefer a map
        return this.keyValueObjectLiteralExpression(node, renderer);
    }
    knownStructObjectLiteralExpression(node, structType, renderer) {
        return new o_tree_1.OTree(['new ', structType.type.symbol.name, ' { '], renderer.convertAll(node.properties), {
            suffix: renderer.mirrorNewlineBefore(node.properties[0], '}', ' '),
            separator: ', ',
            indent: 4,
        });
    }
    keyValueObjectLiteralExpression(node, renderer) {
        // Try to infer an element type from the elements
        const valueType = (0, types_1.inferMapElementType)(node.properties, renderer.typeChecker);
        return new o_tree_1.OTree(['new Dictionary<string, ', this.renderType(node, valueType, false, 'object', renderer), '> { '], renderer.updateContext({ inKeyValueList: true }).convertAll(node.properties), {
            suffix: renderer.mirrorNewlineBefore(node.properties[0], '}', ' '),
            separator: ', ',
            indent: 4,
        });
    }
    shorthandPropertyAssignment(node, renderer) {
        return this.renderPropertyAssignment(node.name, node.name, renderer);
    }
    propertyAssignment(node, renderer) {
        return this.renderPropertyAssignment(node.name, node.initializer, renderer);
    }
    renderPropertyAssignment(key, value, renderer) {
        if (renderer.currentContext.inKeyValueList) {
            return new o_tree_1.OTree([
                '{ ',
                renderer
                    .updateContext({
                    propertyOrMethod: false,
                    identifierAsString: !ts.isComputedPropertyName(key),
                })
                    .convert(key),
                ', ',
                renderer.updateContext({ inKeyValueList: false }).convert(value),
                ' }',
            ], [], { canBreakLine: true });
        }
        return new o_tree_1.OTree([
            renderer.updateContext({ propertyOrMethod: true, stringAsIdentifier: true }).convert(key),
            ' = ',
            renderer.convert(value),
        ], [], { canBreakLine: true });
    }
    arrayLiteralExpression(node, renderer) {
        return new o_tree_1.OTree(['new [] { '], renderer.convertAll(node.elements), {
            separator: ', ',
            suffix: ' }',
            indent: 4,
        });
    }
    ifStatement(node, renderer) {
        const ifStmt = new o_tree_1.OTree(['if (', renderer.convert(node.expression), ') '], [renderer.convert(node.thenStatement)], { canBreakLine: true });
        const elseStmt = node.elseStatement
            ? new o_tree_1.OTree(['else '], [renderer.convert(node.elseStatement)], {
                canBreakLine: true,
            })
            : undefined;
        return elseStmt
            ? new o_tree_1.OTree([], [ifStmt, elseStmt], {
                separator: '\n',
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
        return new o_tree_1.OTree(['for (var ', variableName, ' in ', renderer.convert(node.expression), ') '], [renderer.convert(node.statement)], { canBreakLine: true });
    }
    asExpression(node, context) {
        return new o_tree_1.OTree(['(', this.renderTypeNode(node.type, false, context), ')', context.convert(node.expression)]);
    }
    variableDeclaration(node, renderer) {
        let typeOrVar = 'var';
        const fallback = node.type?.getText() ?? 'var';
        const type = (node.type && renderer.typeOfType(node.type)) ??
            (node.initializer && renderer.typeOfExpression(node.initializer));
        const varType = this.renderType(node, type, false, fallback, renderer);
        // If there is an initializer, and the value isn't "IDictionary<...", we always use var, as this is the
        // recommendation from Roslyn.
        if (varType !== 'object' && (varType.startsWith('IDictionary<') || node.initializer == null)) {
            typeOrVar = varType;
        }
        if (!node.initializer) {
            return new o_tree_1.OTree([typeOrVar, ' ', renderer.convert(node.name), ';']);
        }
        return new o_tree_1.OTree([
            typeOrVar,
            ' ',
            renderer.convert(node.name),
            ' = ',
            renderer.updateContext({ preferObjectLiteralAsStruct: false }).convert(node.initializer),
            ';',
        ], undefined, { canBreakLine: true });
    }
    templateExpression(node, context) {
        // If this is a multi-line string literal, we need not quote much, as @"string" literals in C#
        // do not perform any quoting. The literal quotes in the text however must be doubled.
        const isMultiLine = !!node.head.rawText?.includes('\n') || node.templateSpans.some((span) => span.literal.rawText?.includes('\n'));
        const parts = new Array();
        if (node.head.rawText) {
            parts.push(isMultiLine ? node.head.rawText.replace(/"/g, '""') : (0, ast_utils_1.quoteStringLiteral)(node.head.rawText));
        }
        for (const span of node.templateSpans) {
            parts.push(`{${context.textOf(span.expression)}}`);
            if (span.literal.rawText) {
                parts.push(isMultiLine ? span.literal.rawText.replace(/"/g, '""') : (0, ast_utils_1.quoteStringLiteral)(span.literal.rawText));
            }
        }
        return new o_tree_1.OTree([isMultiLine ? '$@"' : '$"', ...parts, '"']);
    }
    argumentList(args, renderer) {
        return new o_tree_1.OTree([], args ? renderer.updateContext({ preferObjectLiteralAsStruct: true }).convertAll(args) : [], {
            separator: ', ',
        });
    }
    renderTypeNode(typeNode, questionMark, renderer) {
        if (!typeNode) {
            return 'void';
        }
        return this.renderType(typeNode, renderer.typeOfType(typeNode), questionMark, renderer.textOf(typeNode), renderer);
    }
    renderType(typeNode, type, questionMark, fallback, renderer) {
        if (type === undefined) {
            return fallback;
        }
        const renderedType = doRender((0, jsii_types_1.determineJsiiType)(renderer.typeChecker, type));
        const suffix = questionMark || (0, types_1.typeContainsUndefined)(type) ? '?' : '';
        return renderedType + suffix;
        // eslint-disable-next-line consistent-return
        function doRender(jsiiType) {
            switch (jsiiType.kind) {
                case 'unknown':
                    return fallback;
                case 'error':
                    renderer.report(typeNode, jsiiType.message);
                    return fallback;
                case 'map':
                    return `IDictionary<string, ${doRender(jsiiType.elementType)}>`;
                case 'list':
                    return `${doRender(jsiiType.elementType)}[]`;
                case 'namedType':
                    return jsiiType.name;
                case 'builtIn':
                    switch (jsiiType.builtIn) {
                        case 'boolean':
                            return 'boolean';
                        case 'number':
                            return 'int';
                        case 'string':
                            return 'string';
                        case 'any':
                            return 'object';
                        case 'void':
                            return 'void';
                    }
            }
        }
    }
    classHeritage(node, renderer) {
        const heritage = (0, util_1.flat)(Array.from(node.heritageClauses ?? []).map((h) => Array.from(h.types))).map((t) => renderer.convert(t.expression));
        return heritage.length > 0 ? [' : ', new o_tree_1.OTree([], heritage, { separator: ', ' })] : [];
    }
}
exports.CSharpVisitor = CSharpVisitor;
/**
 * Translation version
 *
 * Bump this when you change something in the implementation to invalidate
 * existing cached translations.
 */
CSharpVisitor.VERSION = '1';
/**
 * Uppercase the first letter
 */
function ucFirst(x) {
    return x.slice(0, 1).toUpperCase() + x.slice(1);
}
/**
 * Find the Java name of a module or type
 */
function findDotnetName(jsiiSymbol) {
    if (!jsiiSymbol.sourceAssembly?.assembly) {
        // Don't have accurate info, just guess
        return jsiiSymbol.symbolType !== 'module' ? (0, jsii_utils_1.simpleName)(jsiiSymbol.fqn) : guessDotnetNamespace(jsiiSymbol.fqn);
    }
    const asm = jsiiSymbol.sourceAssembly?.assembly;
    return recurse(jsiiSymbol.fqn);
    function recurse(fqn) {
        if (fqn === asm.name) {
            return (0, packages_1.jsiiTargetParameter)(asm, 'dotnet.namespace') ?? guessDotnetNamespace(fqn);
        }
        if (asm.submodules?.[fqn]) {
            const modName = (0, packages_1.jsiiTargetParameter)(asm.submodules[fqn], 'dotnet.namespace');
            if (modName) {
                return modName;
            }
        }
        return `${recurse((0, jsii_utils_1.namespaceName)(fqn))}.${ucFirst((0, jsii_utils_1.simpleName)(jsiiSymbol.fqn))}`;
    }
}
function guessDotnetNamespace(ref) {
    return ref
        .split(/[^a-zA-Z0-9]+/g)
        .filter((s) => s !== '')
        .map(ucFirst)
        .join('.');
}
//# sourceMappingURL=csharp.js.map