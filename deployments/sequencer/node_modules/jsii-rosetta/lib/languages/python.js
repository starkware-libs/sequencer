"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.PythonVisitor = void 0;
const ts = require("typescript");
const default_1 = require("./default");
const jsii_types_1 = require("../jsii/jsii-types");
const jsii_utils_1 = require("../jsii/jsii-utils");
const packages_1 = require("../jsii/packages");
const target_language_1 = require("../languages/target-language");
const o_tree_1 = require("../o-tree");
const renderer_1 = require("../renderer");
const ast_utils_1 = require("../typescript/ast-utils");
const types_1 = require("../typescript/types");
const util_1 = require("../util");
class PythonVisitor extends default_1.DefaultVisitor {
    constructor(options = {}) {
        super();
        this.options = options;
        this.language = target_language_1.TargetLanguage.PYTHON;
        this.defaultContext = {};
        /**
         * Keep track of module imports we've seen, so that if we need to render a type we can pick from these modules
         */
        this.imports = new Array();
        /**
         * Synthetic imports that need to be added as a final step
         */
        this.syntheticImportsToAdd = new Array();
        this.statementTerminator = '';
    }
    mergeContext(old, update) {
        return Object.assign({}, old, update);
    }
    commentRange(comment, _context) {
        const commentText = (0, ast_utils_1.stripCommentMarkers)(comment.text, comment.kind === ts.SyntaxKind.MultiLineCommentTrivia);
        const hashLines = commentText
            .split('\n')
            .map((l) => `# ${l}`)
            .join('\n');
        const needsAdditionalTrailer = comment.hasTrailingNewLine;
        return new o_tree_1.OTree([comment.isTrailing ? ' ' : '', hashLines, needsAdditionalTrailer ? '\n' : ''], [], {
            // Make sure comment is rendered exactly once in the output tree, no
            // matter how many source nodes it is attached to.
            renderOnce: `comment-${comment.pos}`,
        });
    }
    sourceFile(node, context) {
        let rendered = super.sourceFile(node, context);
        // Add synthetic imports
        if (this.syntheticImportsToAdd.length > 0) {
            rendered = new o_tree_1.OTree([...this.renderSyntheticImports(), rendered]);
        }
        if (this.options.disclaimer) {
            rendered = new o_tree_1.OTree([`# ${this.options.disclaimer}\n`, rendered]);
        }
        return rendered;
    }
    importStatement(node, context) {
        if (node.imports.import === 'full') {
            const moduleName = (0, util_1.fmap)(node.moduleSymbol, findPythonName) ?? guessPythonPackageName(node.packageName);
            const importName = node.imports.alias ?? node.imports.sourceName;
            this.addImport({
                importedFqn: node.moduleSymbol?.fqn ?? node.packageName,
                importName,
            });
            return new o_tree_1.OTree([`import ${moduleName} as ${mangleIdentifier(importName)}`], [], {
                canBreakLine: true,
            });
        }
        if (node.imports.import === 'selective') {
            for (const im of node.imports.elements) {
                if (im.importedSymbol) {
                    this.addImport({
                        importName: im.alias ? im.alias : im.sourceName,
                        importedFqn: im.importedSymbol.fqn,
                    });
                }
            }
            const imports = node.imports.elements.map((im) => {
                const localName = im.alias ?? im.sourceName;
                const originalName = (0, util_1.fmap)((0, util_1.fmap)(im.importedSymbol, findPythonName), jsii_utils_1.simpleName) ?? im.sourceName;
                return localName === originalName
                    ? mangleIdentifier(originalName)
                    : `${mangleIdentifier(originalName)} as ${mangleIdentifier(localName)}`;
            });
            const moduleName = (0, util_1.fmap)(node.moduleSymbol, findPythonName) ?? guessPythonPackageName(node.packageName);
            return new o_tree_1.OTree([`from ${moduleName} import ${imports.join(', ')}`], [], {
                canBreakLine: true,
            });
        }
        return (0, renderer_1.nimpl)(node.node, context);
    }
    token(node, context) {
        const text = context.textOf(node);
        const mapped = TOKEN_REWRITES[text];
        if (mapped) {
            return new o_tree_1.OTree([mapped]);
        }
        return super.token(node, context);
    }
    identifier(node, context) {
        const originalIdentifier = node.text;
        const explodedParameter = context.currentContext.explodedParameter;
        if (context.currentContext.tailPositionArgument &&
            explodedParameter &&
            explodedParameter.type &&
            explodedParameter.variableName === originalIdentifier) {
            return new o_tree_1.OTree([], (0, jsii_utils_1.propertiesOfStruct)(explodedParameter.type, context).map((prop) => new o_tree_1.OTree([prop.name, '=', prop.name])), { separator: ', ' });
        }
        return new o_tree_1.OTree([mangleIdentifier(originalIdentifier)]);
    }
    functionDeclaration(node, context) {
        return this.functionLike(node, context);
    }
    constructorDeclaration(node, context) {
        return this.functionLike(node, context, { isConstructor: true });
    }
    methodDeclaration(node, context) {
        return this.functionLike(node, context);
    }
    expressionStatement(node, context) {
        const text = context.textOf(node);
        if (text === 'true') {
            return new o_tree_1.OTree(['True']);
        }
        if (text === 'false') {
            return new o_tree_1.OTree(['False']);
        }
        return super.expressionStatement(node, context);
    }
    // tslint:disable-next-line:max-line-length
    functionLike(node, context, opts = {}) {
        const methodName = opts.isConstructor ? '__init__' : (0, o_tree_1.renderTree)(context.convert(node.name));
        const [paramDecls, explodedParameter] = this.convertFunctionCallParameters(node.parameters, context);
        const ret = new o_tree_1.OTree([
            'def ',
            methodName,
            '(',
            new o_tree_1.OTree([], [context.currentContext.inClass ? 'self' : undefined, ...paramDecls], {
                separator: ', ',
            }),
            '): ',
        ], [context.updateContext({ explodedParameter, currentMethodName: methodName }).convert(node.body)], {
            canBreakLine: true,
        });
        return ret;
    }
    block(node, context) {
        if (node.statements.length === 0) {
            return new o_tree_1.OTree([], ['\npass'], { indent: 4, canBreakLine: true });
        }
        return new o_tree_1.OTree([], context.convertAll(node.statements), {
            separator: '',
            indent: 4,
            canBreakLine: true,
        });
    }
    regularCallExpression(node, context) {
        let expressionText = context.convert(node.expression);
        if ((0, ast_utils_1.matchAst)(node.expression, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.SuperKeyword)) && context.currentContext.currentMethodName) {
            expressionText = `super().${context.currentContext.currentMethodName}`;
        }
        const signature = context.typeChecker.getResolvedSignature(node);
        return new o_tree_1.OTree([
            expressionText,
            '(',
            this.convertFunctionCallArguments(node.arguments, context, signature?.parameters?.map((p) => p.valueDeclaration)),
            ')',
        ], [], { canBreakLine: true });
    }
    propertyAccessExpression(node, context, submoduleReference) {
        const fullText = context.textOf(node);
        if (fullText in BUILTIN_FUNCTIONS) {
            return new o_tree_1.OTree([BUILTIN_FUNCTIONS[fullText]]);
        }
        const explodedParameter = context.currentContext.explodedParameter;
        // We might be in a context where we've exploded this struct into arguments,
        // in which case we will return just the accessed variable.
        if (explodedParameter && context.textOf(node.expression) === explodedParameter.variableName) {
            return context.convert(node.name);
        }
        if (submoduleReference != null) {
            return context.convert(submoduleReference.lastNode);
        }
        return super.propertyAccessExpression(node, context, submoduleReference);
    }
    parameterDeclaration(node, context) {
        const type = node.type && context.typeOfType(node.type);
        if (context.currentContext.tailPositionParameter &&
            type &&
            (0, jsii_utils_1.analyzeStructType)(context.typeChecker, type) !== false) {
            // Return the parameter that we exploded so that we can use this information
            // while translating the body.
            if (context.currentContext.returnExplodedParameter) {
                context.currentContext.returnExplodedParameter.value = {
                    variableName: context.textOf(node.name),
                    type,
                };
            }
            // Explode to fields
            return new o_tree_1.OTree([], ['*', ...(0, jsii_utils_1.propertiesOfStruct)(type, context).map(renderStructProperty)], { separator: ', ' });
        }
        const suffix = (0, types_1.parameterAcceptsUndefined)(node, type) ? '=None' : '';
        return new o_tree_1.OTree([node.dotDotDotToken ? '*' : '', context.convert(node.name), suffix]);
        function renderStructProperty(prop) {
            const sfx = (0, jsii_utils_1.structPropertyAcceptsUndefined)(prop) ? '=None' : '';
            return prop.name + sfx;
        }
    }
    ifStatement(node, context) {
        const ifStmt = new o_tree_1.OTree(['if ', context.convert(node.expression), ': '], [context.convert(node.thenStatement)], {
            canBreakLine: true,
        });
        const elseStmt = node.elseStatement
            ? new o_tree_1.OTree(['else: '], [context.convert(node.elseStatement)], {
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
    unknownTypeObjectLiteralExpression(node, context) {
        // Neutralize local modifiers if any for transforming further down.
        const downContext = context.updateContext({
            tailPositionArgument: false,
            variadicArgument: false,
        });
        if (context.currentContext.tailPositionArgument && !context.currentContext.variadicArgument) {
            // Guess that it's a struct we can probably inline the kwargs for
            return this.renderObjectLiteralExpression('', '', true, node, downContext);
        }
        return this.renderObjectLiteralExpression('{', '}', false, node, downContext);
    }
    knownStructObjectLiteralExpression(node, structType, context) {
        if (context.currentContext.tailPositionArgument) {
            // We know it's a struct we can DEFINITELY inline the args for
            return this.renderObjectLiteralExpression('', '', true, node, context);
        }
        const structName = structType.kind === 'struct' ? this.importedNameForType(structType.jsiiSym) : structType.type.symbol.name;
        return this.renderObjectLiteralExpression(`${structName}(`, ')', true, node, context);
    }
    keyValueObjectLiteralExpression(node, context) {
        return this.renderObjectLiteralExpression('{', '}', false, node, context.updateContext({ inMap: true }));
    }
    translateUnaryOperator(operator) {
        if (operator === ts.SyntaxKind.ExclamationToken) {
            return 'not ';
        }
        return super.translateUnaryOperator(operator);
    }
    renderObjectLiteralExpression(prefix, suffix, renderObjectLiteralAsKeywords, node, context) {
        return new o_tree_1.OTree([prefix], context.updateContext({ renderObjectLiteralAsKeywords }).convertAll(node.properties), {
            suffix: context.mirrorNewlineBefore(node.properties[0], suffix),
            separator: ', ',
            indent: 4,
        });
    }
    arrayLiteralExpression(node, context) {
        return new o_tree_1.OTree(['['], context.convertAll(node.elements), {
            suffix: context.mirrorNewlineBefore(node.elements[0], ']'),
            separator: ', ',
            indent: 4,
        });
    }
    propertyAssignment(node, context) {
        const mid = context.currentContext.renderObjectLiteralAsKeywords ? '=' : ': ';
        // node.name is either an identifier or a string literal. The string literal
        // needs to be converted differently depending on whether it needs to be a
        // string or a keyword argument.
        let name = ts.isStringLiteral(node.name)
            ? new o_tree_1.OTree([
                context.currentContext.inMap // If in map, don't mangle the keys
                    ? node.name.text
                    : mangleIdentifier(node.name.text),
            ])
            : context.convert(node.name);
        // If this isn't a computed property, we must quote the key (unless it's rendered as a keyword)
        if (context.currentContext.inMap ||
            (!context.currentContext.renderObjectLiteralAsKeywords && !ts.isComputedPropertyName(node.name))) {
            name = new o_tree_1.OTree(['"', name, '"']);
        }
        return new o_tree_1.OTree([name, mid, context.updateContext({ inMap: false, tailPositionArgument: false }).convert(node.initializer)], [], { canBreakLine: true });
    }
    shorthandPropertyAssignment(node, context) {
        let before = '"';
        let mid = '": ';
        if (context.currentContext.renderObjectLiteralAsKeywords) {
            before = '';
            mid = '=';
        }
        return new o_tree_1.OTree([before, context.convert(node.name), mid, context.convert(node.name)], [], { canBreakLine: true });
    }
    newExpression(node, context) {
        return new o_tree_1.OTree([context.convert(node.expression), '(', this.convertFunctionCallArguments(node.arguments, context), ')'], [], { canBreakLine: true });
    }
    variableDeclaration(node, context) {
        let fallback = 'object';
        if (node.type) {
            fallback = node.type.getText();
        }
        if (!node.initializer) {
            const type = (node.type && context.typeOfType(node.type)) ||
                (node.initializer && context.typeOfExpression(node.initializer));
            const renderedType = type ? this.renderType(node, type, context, fallback) : fallback;
            return new o_tree_1.OTree(['# ', context.convert(node.name), ': ', renderedType], []);
        }
        return new o_tree_1.OTree([context.convert(node.name), ' = ', context.convert(node.initializer)], [], {
            canBreakLine: true,
        });
    }
    thisKeyword() {
        return new o_tree_1.OTree(['self']);
    }
    forOfStatement(node, context) {
        // This is what a "for (const x of ...)" looks like in the AST
        let variableName = '???';
        (0, ast_utils_1.matchAst)(node.initializer, (0, ast_utils_1.nodeOfType)(ts.SyntaxKind.VariableDeclarationList, (0, ast_utils_1.nodeOfType)('var', ts.SyntaxKind.VariableDeclaration)), (bindings) => {
            variableName = mangleIdentifier(context.textOf(bindings.var.name));
        });
        return new o_tree_1.OTree(['for ', variableName, ' in ', context.convert(node.expression), ': '], [context.convert(node.statement)], { canBreakLine: true });
    }
    classDeclaration(node, context) {
        const allHeritageClauses = Array.from(node.heritageClauses ?? []).flatMap((h) => Array.from(h.types));
        // List of booleans matching `allHeritage` array
        const isJsii = allHeritageClauses.map((e) => (0, util_1.fmap)(context.typeOfExpression(e.expression), (type) => (0, jsii_utils_1.isJsiiProtocolType)(context.typeChecker, type)) ?? false);
        const jsiiImplements = allHeritageClauses.filter((_, i) => isJsii[i]);
        const inlineHeritage = allHeritageClauses.filter((_, i) => !isJsii[i]);
        const hasHeritage = inlineHeritage.length > 0;
        const members = context.updateContext({ inClass: true }).convertAll(node.members);
        if (members.length === 0) {
            members.push(new o_tree_1.OTree(['\npass'], []));
        }
        const ret = new o_tree_1.OTree([
            ...jsiiImplements.flatMap((i) => ['@jsii.implements(', context.convert(i.expression), ')\n']),
            'class ',
            node.name ? context.textOf(node.name) : '???',
            hasHeritage ? '(' : '',
            ...inlineHeritage.map((t) => context.convert(t.expression)),
            hasHeritage ? ')' : '',
            ': ',
        ], members, {
            indent: 4,
            canBreakLine: true,
        });
        return ret;
    }
    printStatement(args, context) {
        return new o_tree_1.OTree(['print', '(', new o_tree_1.OTree([], context.convertAll(args), { separator: ', ' }), ')']);
    }
    propertyDeclaration(_node, _context) {
        return new o_tree_1.OTree([]);
    }
    /**
     * We have to do something special here
     *
     * Best-effort, we remember the fields of struct interfaces and keep track of
     * them. Fortunately we can determine from the name whether what to do.
     */
    interfaceDeclaration(_node, _context) {
        // Whatever we do, nothing here will have a representation
        return o_tree_1.NO_SYNTAX;
    }
    propertySignature(_node, _context) {
        // Does not represent in Python
        return o_tree_1.NO_SYNTAX;
    }
    methodSignature(_node, _context) {
        // Does not represent in Python
        return o_tree_1.NO_SYNTAX;
    }
    asExpression(node, context) {
        return context.convert(node.expression);
    }
    stringLiteral(node, _context) {
        if (node.getText(node.getSourceFile()).includes('\n')) {
            return new o_tree_1.OTree([
                '"""',
                node.text
                    // Escape all occurrences of back-slash once more
                    .replace(/\\/g, '\\\\')
                    // Escape only the first one in triple-quotes
                    .replace(/"""/g, '\\"""'),
                '"""',
            ]);
        }
        return new o_tree_1.OTree([JSON.stringify(node.text)]);
    }
    templateExpression(node, context) {
        const parts = new Array();
        if (node.head.rawText) {
            parts.push((0, ast_utils_1.quoteStringLiteral)(node.head.rawText));
        }
        for (const span of node.templateSpans) {
            parts.push(`{${context.textOf(span.expression)}}`);
            if (span.literal.rawText) {
                parts.push((0, ast_utils_1.quoteStringLiteral)(span.literal.rawText));
            }
        }
        const quote = parts.some((part) => part.includes('\n')) ? '"""' : '"';
        return new o_tree_1.OTree([`f${quote}`, ...parts, quote]);
    }
    maskingVoidExpression(node, _context) {
        const arg = (0, ast_utils_1.voidExpressionString)(node);
        if (arg === 'block') {
            return new o_tree_1.OTree(['# ...'], [], { canBreakLine: true });
        }
        if (arg === '...') {
            return new o_tree_1.OTree(['...']);
        }
        return o_tree_1.NO_SYNTAX;
    }
    /**
     * Convert parameters
     *
     * If the last one has the type of a known struct, explode to keyword-only arguments.
     *
     * Returns a pair of [decls, excploded-var-name].
     */
    // tslint:disable-next-line:max-line-length
    convertFunctionCallParameters(params, context) {
        if (!params || params.length === 0) {
            return [[], undefined];
        }
        const returnExplodedParameter = {};
        // Convert the last element differently
        const converted = params.length > 0
            ? [
                ...context.convertAll(params.slice(0, params.length - 1)),
                context
                    .updateContext({
                    tailPositionParameter: true,
                    returnExplodedParameter,
                })
                    .convert(last(params)),
            ]
            : [];
        return [converted, returnExplodedParameter.value];
    }
    /**
     * Convert arguments.
     *
     * If the last argument:
     *
     * - is an object literal, explode it.
     * - is itself an exploded argument in our call signature, explode the fields
     */
    convertFunctionCallArguments(args, context, parameterDeclarations) {
        if (!args) {
            return o_tree_1.NO_SYNTAX;
        }
        const converted = context.convertWithModifier(args, (ctx, _arg, index) => {
            const decl = parameterDeclarations?.[Math.min(index, parameterDeclarations.length - 1)];
            const variadicArgument = decl?.dotDotDotToken != null;
            const tailPositionArgument = index >= args.length - 1;
            return ctx.updateContext({ variadicArgument, tailPositionArgument });
        });
        return new o_tree_1.OTree([], converted, { separator: ', ', indent: 4 });
    }
    /**
     * Render a type.
     *
     * Not usually a thing in Python, but useful for declared variables.
     */
    renderType(owningNode, type, renderer, fallback) {
        return doRender((0, jsii_types_1.determineJsiiType)(renderer.typeChecker, type));
        // eslint-disable-next-line consistent-return
        function doRender(jsiiType) {
            switch (jsiiType.kind) {
                case 'unknown':
                    return fallback;
                case 'error':
                    renderer.report(owningNode, jsiiType.message);
                    return fallback;
                case 'map':
                    return `Dict[str, ${doRender(jsiiType.elementType)}]`;
                case 'list':
                    return `List[${doRender(jsiiType.elementType)}]`;
                case 'namedType':
                    // in this case, the fallback will hold more information than jsiiType.name
                    return fallback;
                case 'builtIn':
                    switch (jsiiType.builtIn) {
                        case 'boolean':
                            return 'bool';
                        case 'number':
                            return 'number';
                        case 'string':
                            return 'str';
                        case 'any':
                            return 'Any';
                        default:
                            return jsiiType.builtIn;
                    }
            }
        }
    }
    addImport(x) {
        this.imports.push(x);
        // Sort in reverse order of FQN length
        (0, util_1.sortBy)(this.imports, (i) => [-i.importedFqn.length]);
    }
    /**
     * Find the import for the FQNs submodule, and return it and the rest of the name
     */
    importedNameForType(jsiiSym) {
        // Look for an existing import that contains this symbol
        for (const imp of this.imports) {
            if (jsiiSym.fqn.startsWith(`${imp.importedFqn}.`)) {
                const remainder = jsiiSym.fqn.substring(imp.importedFqn.length + 1);
                return `${imp.importName}.${remainder}`;
            }
        }
        // Otherwise look up the Python name of this symbol (but not for fake imports from tests)
        const pythonName = findPythonName(jsiiSym);
        if (!jsiiSym.fqn.startsWith('fake_jsii.') && pythonName) {
            this.syntheticImportsToAdd.push(pythonName);
        }
        return (0, jsii_utils_1.simpleName)(jsiiSym.fqn);
    }
    renderSyntheticImports() {
        const grouped = (0, util_1.groupBy)(this.syntheticImportsToAdd, jsii_utils_1.namespaceName);
        return Object.entries(grouped).map(([namespaceFqn, fqns]) => {
            const simpleNames = fqns.map(jsii_utils_1.simpleName);
            return `from ${namespaceFqn} import ${simpleNames.join(', ')}\n`;
        });
    }
}
exports.PythonVisitor = PythonVisitor;
/**
 * Translation version
 *
 * Bump this when you change something in the implementation to invalidate
 * existing cached translations.
 */
PythonVisitor.VERSION = '2';
function mangleIdentifier(originalIdentifier) {
    if ((0, util_1.startsWithUppercase)(originalIdentifier)) {
        // Probably a class, leave as-is
        return originalIdentifier;
    }
    // Turn into snake-case
    const cased = originalIdentifier.replace(/[^A-Z][A-Z]/g, (m) => `${m[0].slice(0, 1)}_${m.slice(1).toLowerCase()}`);
    return IDENTIFIER_KEYWORDS.includes(cased) ? `${cased}_` : cased;
}
const BUILTIN_FUNCTIONS = {
    'console.log': 'print',
    'console.error': 'sys.stderr.write',
    'Math.random': 'random.random',
};
const TOKEN_REWRITES = {
    this: 'self',
    true: 'True',
    false: 'False',
};
const IDENTIFIER_KEYWORDS = ['lambda'];
function last(xs) {
    return xs[xs.length - 1];
}
/**
 * Find the Python name of a module or type
 */
function findPythonName(jsiiSymbol) {
    if (!jsiiSymbol.sourceAssembly?.assembly) {
        // Don't have accurate info, just guess
        return jsiiSymbol.symbolType !== 'module' ? (0, jsii_utils_1.simpleName)(jsiiSymbol.fqn) : guessPythonPackageName(jsiiSymbol.fqn);
    }
    const asm = jsiiSymbol.sourceAssembly?.assembly;
    return recurse(jsiiSymbol.fqn);
    function recurse(fqn) {
        if (fqn === asm.name) {
            return (0, packages_1.jsiiTargetParameter)(asm, 'python.module') ?? guessPythonPackageName(fqn);
        }
        if (asm.submodules?.[fqn]) {
            const modName = (0, packages_1.jsiiTargetParameter)(asm.submodules[fqn], 'python.module');
            if (modName) {
                return modName;
            }
        }
        return `${recurse((0, jsii_utils_1.namespaceName)(fqn))}.${(0, jsii_utils_1.simpleName)(jsiiSymbol.fqn)}`;
    }
}
/**
 * Pythonify an assembly name and hope it is correct
 */
function guessPythonPackageName(ref) {
    return ref.replace(/^@/, '').replace(/\//g, '.').replace(/-/g, '_');
}
//# sourceMappingURL=python.js.map