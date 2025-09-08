"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoVisitor = void 0;
// import { JsiiSymbol, simpleName, namespaceName } from '../jsii/jsii-utils';
// import { jsiiTargetParameter } from '../jsii/packages';
const node_assert_1 = require("node:assert");
const ts = require("typescript");
const default_1 = require("./default");
const target_language_1 = require("./target-language");
const jsii_types_1 = require("../jsii/jsii-types");
const jsii_utils_1 = require("../jsii/jsii-utils");
const o_tree_1 = require("../o-tree");
const ast_utils_1 = require("../typescript/ast-utils");
const imports_1 = require("../typescript/imports");
const types_1 = require("../typescript/types");
var DeclarationType;
(function (DeclarationType) {
    DeclarationType[DeclarationType["STRUCT"] = 0] = "STRUCT";
    DeclarationType[DeclarationType["INTERFACE"] = 1] = "INTERFACE";
    DeclarationType[DeclarationType["FUNCTION"] = 2] = "FUNCTION";
    DeclarationType[DeclarationType["BUILTIN"] = 3] = "BUILTIN";
    DeclarationType[DeclarationType["UNKNOWN"] = 4] = "UNKNOWN";
})(DeclarationType || (DeclarationType = {}));
class GoVisitor extends default_1.DefaultVisitor {
    constructor() {
        super(...arguments);
        this.indentChar = '\t';
        this.language = target_language_1.TargetLanguage.GO;
        this.idMap = new Map();
        this.defaultContext = {
            isExported: false,
            isPtr: false,
            isPtrAssignmentRValue: false,
            isStruct: false,
            isInterface: false,
            isParameterName: false,
            inMapLiteral: false,
            wrapPtr: false,
        };
    }
    argumentList(args, renderer) {
        return new o_tree_1.OTree([], args ? renderer.convertAll(args) : [], {
            separator: ', ',
        });
    }
    block(node, renderer) {
        return new o_tree_1.OTree(['{'], renderer.convertAll(node.statements), {
            indent: 1,
            suffix: renderer.mirrorNewlineBefore(node.statements[0], '}'),
        });
    }
    expressionStatement(node, renderer) {
        const inner = renderer.convert(node.expression);
        if (inner.isEmpty) {
            return inner;
        }
        return new o_tree_1.OTree([inner], [], { canBreakLine: true });
    }
    functionDeclaration(node, renderer) {
        const funcName = renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node) }).convert(node.name);
        const returnType = (0, types_1.determineReturnType)(renderer.typeChecker, node);
        const goType = this.renderType(node.type ?? node, returnType?.symbol, returnType, true, '', renderer);
        const body = node.body?.statements ? renderer.convertAll(node.body.statements) : [];
        return new o_tree_1.OTree([
            'func ',
            funcName,
            '(',
            new o_tree_1.OTree([], renderer.updateContext({ isPtr: true }).convertAll(node.parameters), {
                separator: ', ',
            }),
            ')',
            goType ? ' ' : '',
            goType,
        ], [
            new o_tree_1.OTree([' {'], [this.defaultArgValues(node.parameters, renderer.updateContext({ wrapPtr: true })), ...body], {
                indent: 1,
                suffix: '\n}',
            }),
        ], {
            canBreakLine: true,
        });
    }
    identifier(node, renderer) {
        const symbol = renderer.typeChecker.getSymbolAtLocation(node);
        // If the identifier corresponds to a renamed imported symbol, we need to use the original symbol name, qualified
        // with the import package name, since Go does not allow generalized symbol aliasing (we *could* alias types, but
        // not static functions or constructors).
        const declaration = symbol?.valueDeclaration ?? symbol?.declarations?.[0];
        if (declaration && ts.isImportSpecifier(declaration)) {
            const importInfo = (0, imports_1.analyzeImportDeclaration)(declaration.parent.parent.parent, renderer);
            const packageName = importInfo.moduleSymbol?.sourceAssembly?.packageJson.jsii?.targets?.go?.packageName ??
                this.goName(importInfo.packageName, renderer, undefined);
            const importedSymbol = declaration.propertyName
                ? renderer.typeChecker.getSymbolAtLocation(declaration.propertyName)
                : symbol;
            // Note: imported members are (by nature) always exported by the module they are imported from.
            return new o_tree_1.OTree([
                packageName,
                '.',
                this.goName((declaration.propertyName ?? declaration.name).text, renderer.updateContext({ isExported: true }), importedSymbol),
            ]);
        }
        return new o_tree_1.OTree([this.goName(node.text, renderer, symbol)]);
    }
    newExpression(node, renderer) {
        const { classNamespace, className } = determineClassName.call(this, node.expression);
        return new o_tree_1.OTree([
            ...(classNamespace ? [classNamespace, '.'] : []),
            'New', // Should this be "new" if the class is unexported?
            className,
            '(',
        ], renderer.updateContext({ wrapPtr: true }).convertAll(node.arguments ?? []), { canBreakLine: true, separator: ', ', suffix: ')' });
        function determineClassName(expr) {
            if (ts.isIdentifier(expr)) {
                // Imported names are referred to by the original (i.e: exported) name, qualified with the source module's go
                // package name.
                const symbol = renderer.typeChecker.getSymbolAtLocation(expr);
                const declaration = symbol?.valueDeclaration ?? symbol?.declarations?.[0];
                if (declaration && ts.isImportSpecifier(declaration)) {
                    const importInfo = (0, imports_1.analyzeImportDeclaration)(declaration.parent.parent.parent, renderer);
                    const packageName = importInfo.moduleSymbol?.sourceAssembly?.packageJson.jsii?.targets?.go?.packageName ??
                        this.goName(importInfo.packageName, renderer, undefined);
                    return {
                        classNamespace: new o_tree_1.OTree([packageName]),
                        className: this.goName((declaration.propertyName ?? declaration.name).text, renderer.updateContext({ isExported: true }), symbol),
                    };
                }
                return { className: ucFirst(expr.text) };
            }
            if (ts.isPropertyAccessExpression(expr)) {
                if (ts.isIdentifier(expr.expression)) {
                    return {
                        className: ucFirst(expr.name.text),
                        classNamespace: renderer.updateContext({ isExported: false }).convert(expr.expression),
                    };
                }
                else if (ts.isPropertyAccessExpression(expr.expression) &&
                    renderer.submoduleReferences.has(expr.expression)) {
                    const submodule = renderer.submoduleReferences.get(expr.expression);
                    return {
                        className: ucFirst(expr.name.text),
                        classNamespace: renderer.updateContext({ isExported: false }).convert(submodule.lastNode),
                    };
                }
                renderer.reportUnsupported(expr.expression, target_language_1.TargetLanguage.GO);
                return {
                    className: ucFirst(expr.name.text),
                    classNamespace: new o_tree_1.OTree(['#error#']),
                };
            }
            renderer.reportUnsupported(expr, target_language_1.TargetLanguage.GO);
            return { className: expr.getText(expr.getSourceFile()) };
        }
    }
    arrayLiteralExpression(node, renderer) {
        const arrayType = (0, types_1.inferredTypeOfExpression)(renderer.typeChecker, node) ?? renderer.typeChecker.getTypeAtLocation(node);
        const [elementType] = renderer.typeChecker.getTypeArguments(arrayType);
        const typeName = elementType
            ? this.renderType(node, elementType.symbol, elementType, true, 'interface{}', renderer)
            : 'interface{}';
        return new o_tree_1.OTree(['[]', typeName, '{'], renderer.convertAll(node.elements), {
            separator: ',',
            trailingSeparator: true,
            suffix: '}',
            indent: 1,
        });
    }
    objectLiteralExpression(node, renderer) {
        const lit = (0, jsii_types_1.analyzeObjectLiteral)(renderer.typeChecker, node);
        switch (lit.kind) {
            case 'unknown':
                return this.unknownTypeObjectLiteralExpression(node, renderer);
            case 'struct':
            case 'local-struct':
                return this.knownStructObjectLiteralExpression(node, lit, renderer);
            case 'map':
                return this.keyValueObjectLiteralExpression(node, renderer);
        }
    }
    propertyAssignment(node, renderer) {
        const key = ts.isStringLiteralLike(node.name) || ts.isIdentifier(node.name)
            ? renderer.currentContext.inMapLiteral
                ? JSON.stringify(node.name.text)
                : this.goName(node.name.text, renderer, renderer.typeChecker.getSymbolAtLocation(node.name))
            : renderer.convert(node.name);
        return new o_tree_1.OTree([
            key,
            ': ',
            renderer
                .updateContext({
                // Reset isExported, as this was intended for the key name translation...
                isExported: undefined,
                // Struct member values are always pointers...
                isPtr: renderer.currentContext.isStruct,
                wrapPtr: renderer.currentContext.isStruct || renderer.currentContext.inMapLiteral,
            })
                .convert(node.initializer),
        ], [], {
            canBreakLine: true,
        });
    }
    shorthandPropertyAssignment(node, renderer) {
        const key = ts.isStringLiteralLike(node.name) || ts.isIdentifier(node.name)
            ? renderer.currentContext.inMapLiteral
                ? JSON.stringify(node.name.text)
                : this.goName(node.name.text, renderer, renderer.typeChecker.getSymbolAtLocation(node.name))
            : renderer.convert(node.name);
        const rawValue = renderer.updateContext({ wrapPtr: true, isStruct: false }).convert(node.name);
        const value = isPointerValue(renderer.typeChecker, node.name)
            ? rawValue
            : wrapPtrExpression(renderer.typeChecker, node.name, rawValue);
        return new o_tree_1.OTree([key, ': ', value], [], { canBreakLine: true });
    }
    templateExpression(node, renderer) {
        let template = '';
        const parameters = new Array();
        if (node.head.rawText) {
            template += node.head.rawText;
        }
        for (const span of node.templateSpans) {
            template += '%v';
            parameters.push(renderer.convert(span.expression));
            if (span.literal.rawText) {
                template += span.literal.rawText;
            }
        }
        if (parameters.length === 0) {
            return new o_tree_1.OTree([JSON.stringify(template)]);
        }
        return new o_tree_1.OTree(['fmt.Sprintf('], [
            JSON.stringify(template),
            ...parameters.reduce((list, element) => list.concat(', ', element), new Array()),
        ], {
            canBreakLine: true,
            suffix: ')',
        });
    }
    token(node, renderer) {
        switch (node.kind) {
            case ts.SyntaxKind.FalseKeyword:
            case ts.SyntaxKind.TrueKeyword:
                if (renderer.currentContext.wrapPtr) {
                    return new o_tree_1.OTree(['jsii.Boolean(', node.getText(), ')']);
                }
                return new o_tree_1.OTree([node.getText()]);
            case ts.SyntaxKind.NullKeyword:
            case ts.SyntaxKind.UndefinedKeyword:
                return new o_tree_1.OTree(['nil']);
            default:
                return super.token(node, renderer);
        }
    }
    unknownTypeObjectLiteralExpression(node, renderer) {
        return this.keyValueObjectLiteralExpression(node, renderer);
    }
    keyValueObjectLiteralExpression(node, renderer) {
        const valueType = (0, types_1.inferMapElementType)(node.properties, renderer.typeChecker);
        return new o_tree_1.OTree([`map[string]`, this.renderType(node, valueType?.symbol, valueType, true, `interface{}`, renderer), `{`], renderer.updateContext({ inMapLiteral: true, wrapPtr: true }).convertAll(node.properties), {
            suffix: '}',
            separator: ',',
            trailingSeparator: true,
            indent: 1,
        });
    }
    knownStructObjectLiteralExpression(node, structType, renderer) {
        const exported = structType.kind === 'struct';
        return new o_tree_1.OTree([
            '&',
            this.goName(structType.type.symbol.name, renderer.updateContext({ isExported: exported, isPtr: false }), structType.type.symbol),
            '{',
        ], renderer.updateContext({ isExported: exported, isStruct: true }).convertAll(node.properties), {
            suffix: '}',
            separator: ',',
            trailingSeparator: true,
            indent: 1,
        });
    }
    asExpression(node, renderer) {
        const jsiiType = (0, jsii_types_1.determineJsiiType)(renderer.typeChecker, renderer.typeChecker.getTypeFromTypeNode(node.type));
        switch (jsiiType.kind) {
            case 'builtIn':
                switch (jsiiType.builtIn) {
                    case 'boolean':
                        return new o_tree_1.OTree(['bool(', renderer.convert(node.expression), ')'], [], { canBreakLine: true });
                    case 'number':
                        return new o_tree_1.OTree(['f64(', renderer.convert(node.expression), ')'], [], { canBreakLine: true });
                    case 'string':
                        return new o_tree_1.OTree(['string(', renderer.convert(node.expression), ')'], [], { canBreakLine: true });
                    case 'any':
                    case 'void':
                        // Just return the value as-is... Everything is compatible with `interface{}`.
                        return renderer.convert(node.expression);
                }
                // To make linter understand there is no fall-through here...
                throw new node_assert_1.AssertionError({ message: 'unreachable' });
            default:
                return new o_tree_1.OTree([renderer.convert(node.expression), '.(', this.renderTypeNode(node.type, false, renderer), ')'], [], { canBreakLine: true });
        }
    }
    parameterDeclaration(node, renderer) {
        const nodeName = renderer.updateContext({ isParameterName: true, isPtr: false }).convert(node.name);
        const nodeType = node.dotDotDotToken ? node.type?.elementType : node.type;
        const typeNode = this.renderTypeNode(nodeType, true, renderer);
        return new o_tree_1.OTree([...(node.dotDotDotToken ? ['...'] : []), nodeName, ' ', typeNode]);
    }
    printStatement(args, renderer) {
        const renderedArgs = this.argumentList(args, renderer);
        return new o_tree_1.OTree(['fmt.Println(', renderedArgs, ')']);
    }
    propertyAccessExpression(node, renderer, submoduleReference) {
        if (submoduleReference != null) {
            return new o_tree_1.OTree([
                renderer
                    .updateContext({ isExported: false, isPtr: false, wrapPtr: false })
                    .convert(submoduleReference.lastNode),
            ]);
        }
        const expressionType = (0, types_1.typeOfExpression)(renderer.typeChecker, node.expression);
        const valueSymbol = renderer.typeChecker.getSymbolAtLocation(node.name);
        const isStaticMember = valueSymbol?.valueDeclaration != null && (0, ast_utils_1.isStatic)(valueSymbol.valueDeclaration);
        const isClassStaticPropertyAccess = isStaticMember &&
            expressionType?.symbol?.valueDeclaration != null &&
            valueSymbol.valueDeclaration != null &&
            ts.isClassDeclaration(expressionType.symbol.valueDeclaration) &&
            (ts.isPropertyDeclaration(valueSymbol.valueDeclaration) || ts.isAccessor(valueSymbol.valueDeclaration));
        const isClassStaticMethodAccess = isStaticMember &&
            !isClassStaticPropertyAccess &&
            valueSymbol.valueDeclaration != null &&
            ts.isMethodDeclaration(valueSymbol.valueDeclaration);
        // When the expression has an unknown type (unresolved symbol), has an upper-case first letter,
        // and doesn't end in a call expression (as hinted by the presence of parentheses), we assume
        // it's a type name... In such cases, what comes after can be considered a static member access.
        // Note that the expression might be further qualified, so we check using a regex that checks
        // for the last "." - delimited segment if there's dots in there...
        const expressionLooksLikeTypeReference = expressionType.symbol == null &&
            /(?:\.|^)[A-Z][^.)]*$/.exec(node.expression.getText(node.expression.getSourceFile())) != null;
        // Whether the node is an enum member reference.
        const isEnumMember = expressionType?.symbol?.valueDeclaration != null && ts.isEnumDeclaration(expressionType.symbol.valueDeclaration);
        const jsiiSymbol = (0, jsii_utils_1.lookupJsiiSymbolFromNode)(renderer.typeChecker, node.name);
        const isExportedTypeName = jsiiSymbol != null && jsiiSymbol.symbolType !== 'module';
        const delimiter = isEnumMember || isClassStaticPropertyAccess || isClassStaticMethodAccess || expressionLooksLikeTypeReference
            ? '_'
            : '.';
        return new o_tree_1.OTree([
            renderer.convert(node.expression),
            delimiter,
            renderer
                .updateContext({
                isExported: isClassStaticPropertyAccess ||
                    isClassStaticMethodAccess ||
                    expressionLooksLikeTypeReference ||
                    isEnumMember ||
                    isExportedTypeName,
            })
                .convert(node.name),
            ...(isClassStaticPropertyAccess
                ? ['()']
                : // If the parent's not a call-like expression, and it's an inferred static property access, we need to put call
                    // parentheses at the end, as static properties are accessed via synthetic readers.
                    expressionLooksLikeTypeReference && findUp(node, ts.isCallLikeExpression) == null
                        ? ['()']
                        : []),
        ]);
    }
    methodSignature(node, renderer) {
        const type = this.renderTypeNode(node.type, true, renderer);
        return new o_tree_1.OTree([
            renderer.updateContext({ isExported: renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node) }).convert(node.name),
            '(',
        ], renderer.convertAll(node.parameters), { suffix: `) ${type}`, canBreakLine: true });
    }
    propertyDeclaration(node, renderer) {
        return new o_tree_1.OTree([
            renderer
                .updateContext({ isExported: (renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node)) || (0, ast_utils_1.isStatic)(node) })
                .convert(node.name),
            ' ',
            this.renderTypeNode(node.type, true, renderer),
        ], [], { canBreakLine: true });
    }
    propertySignature(node, renderer) {
        if (renderer.currentContext.isInterface) {
            const type = this.renderTypeNode(node.type, true, renderer);
            const getter = new o_tree_1.OTree([
                renderer.updateContext({ isExported: renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node) }).convert(node.name),
                '() ',
                type,
            ]);
            if ((0, ast_utils_1.isReadOnly)(node)) {
                return getter;
            }
            const setter = new o_tree_1.OTree([
                '\n',
                renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node) ? 'Set' : 'set',
                renderer.updateContext({ isExported: true }).convert(node.name),
                '(value ',
                type,
                ')',
            ]);
            return new o_tree_1.OTree([getter, setter]);
        }
        return new o_tree_1.OTree([
            '\n',
            renderer.updateContext({ isExported: renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node) }).convert(node.name),
            ' ',
            this.renderTypeNode(node.type, renderer.currentContext.isPtr, renderer),
        ]);
    }
    regularCallExpression(node, renderer) {
        return new o_tree_1.OTree([
            renderer.convert(node.expression),
            '(',
            this.argumentList(node.arguments, renderer.updateContext({ wrapPtr: true })),
            ')',
        ]);
    }
    returnStatement(node, renderer) {
        return new o_tree_1.OTree(['return ', renderer.updateContext({ wrapPtr: true }).convert(node.expression)], [], {
            canBreakLine: true,
        });
    }
    binaryExpression(node, renderer) {
        if (node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
            const symbol = symbolFor(renderer.typeChecker, node.left);
            return new o_tree_1.OTree([
                renderer.convert(node.left),
                ' = ',
                renderer
                    .updateContext({
                    isPtrAssignmentRValue: symbol?.valueDeclaration &&
                        (ts.isParameter(symbol.valueDeclaration) || ts.isPropertyDeclaration(symbol.valueDeclaration)),
                })
                    .convert(node.right),
            ]);
        }
        const output = super.binaryExpression(node, renderer.updateContext({ wrapPtr: false, isPtr: false }));
        if (!renderer.currentContext.wrapPtr) {
            return output;
        }
        return wrapPtrExpression(renderer.typeChecker, node, output);
    }
    stringLiteral(node, renderer) {
        // Go supports backtick-delimited multi-line string literals, similar/same as JavaScript no-substitution templates.
        // We only use this trick if the literal includes actual new line characters (otherwise it just looks weird in go).
        const text = ts.isNoSubstitutionTemplateLiteral(node) && /[\n\r]/m.test(node.text)
            ? node.getText(node.getSourceFile())
            : JSON.stringify(node.text);
        return new o_tree_1.OTree([`${renderer.currentContext.wrapPtr ? jsiiStr(text) : text}`]);
    }
    numericLiteral(node, renderer) {
        const text = `${node.text}`;
        return new o_tree_1.OTree([`${renderer.currentContext.wrapPtr ? jsiiNum(text) : text}`]);
    }
    classDeclaration(node, renderer) {
        const className = node.name
            ? renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node) }).convert(node.name)
            : 'anonymous';
        const extendsClause = node.heritageClauses?.find((clause) => clause.token === ts.SyntaxKind.ExtendsKeyword);
        const base = extendsClause && this.renderTypeNode(extendsClause.types[0], false, renderer);
        const properties = node.members
            .filter(ts.isPropertyDeclaration)
            .map((prop) => renderer.updateContext({ isStruct: true, isPtr: true }).convert(prop));
        const struct = new o_tree_1.OTree(['type ', className, ' struct {'], [...(base ? ['\n', base] : []), ...properties], {
            canBreakLine: true,
            suffix: properties.length > 0 ? renderer.mirrorNewlineBefore(node.members[0], '}') : '\n}',
            indent: 1,
        });
        const methods = [
            node.members.length > 0
                ? // Ensure there is a blank line between thre struct and the first member, but don't put two if there's already
                    // one as part of the first member's leading trivia.
                    new o_tree_1.OTree(['\n\n'], [], { renderOnce: `ws-${node.members[0].getFullStart()}` })
                : '',
            ...renderer.convertAll(node.members.filter((member) => !ts.isPropertyDeclaration(member) || ((0, ast_utils_1.isExported)(node) && !(0, ast_utils_1.isPrivate)(member)))),
        ];
        return new o_tree_1.OTree([struct], methods, { canBreakLine: true });
    }
    structInterfaceDeclaration(node, renderer) {
        const bases = node.heritageClauses?.flatMap((hc) => hc.types).map((t) => this.renderTypeNode(t, false, renderer)) ?? [];
        return new o_tree_1.OTree(['type ', renderer.updateContext({ isStruct: true }).convert(node.name), ' struct {'], [...bases, ...renderer.updateContext({ isStruct: true, isPtr: true }).convertAll(node.members)], { indent: 1, canBreakLine: true, separator: '\n', trailingSeparator: true, suffix: '}' });
    }
    regularInterfaceDeclaration(node, renderer) {
        if (node.members.length === 0) {
            // Erase empty interfaces as they have no bearing in Go
            return new o_tree_1.OTree([]);
        }
        const symbol = renderer.typeChecker.getSymbolAtLocation(node.name);
        const name = this.goName(node.name.text, renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node) }), symbol);
        return new o_tree_1.OTree([`type ${name} interface {`], renderer.updateContext({ isInterface: true, isExported: (0, ast_utils_1.isExported)(node) }).convertAll(node.members), { indent: 1, canBreakLine: true, separator: '\n', trailingSeparator: true, suffix: '}' });
    }
    constructorDeclaration(node, renderer) {
        const className = node.parent.name
            ? this.goName(node.parent.name.text, renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node.parent) }), renderer.typeChecker.getSymbolAtLocation(node.parent.name))
            : 'anonymous';
        const defaultArgValues = this.defaultArgValues(node.parameters, renderer);
        return new o_tree_1.OTree([
            'func ',
            (0, ast_utils_1.isExported)(node.parent) ? 'New' : 'new',
            ucFirst(className),
            '(',
            new o_tree_1.OTree([], renderer.convertAll(node.parameters), { separator: ', ' }),
            ') *',
            className,
            ' {',
            new o_tree_1.OTree([], [defaultArgValues, '\nthis := &', className, '{}'], {
                indent: 1,
            }),
        ], node.body ? renderer.convertAll(node.body.statements) : [], { canBreakLine: true, suffix: '\n\treturn this\n}', indent: 1 });
    }
    superCallExpression(node, renderer) {
        // We're on a `super` call, so we must be extending a base class.
        const base = findUp(node, ts.isConstructorDeclaration).parent.heritageClauses.find((clause) => clause.token === ts.SyntaxKind.ExtendsKeyword).types[0].expression;
        const baseConstructor = ts.isPropertyAccessExpression(base)
            ? new o_tree_1.OTree([
                renderer.convert(base.expression),
                '.New',
                ucFirst(this.goName(base.name.text, renderer, renderer.typeChecker.getSymbolAtLocation(base.name))),
            ])
            : ts.isIdentifier(base)
                ? `new${ucFirst(this.goName(base.text, renderer, renderer.typeChecker.getSymbolAtLocation(base)))}`
                : (function () {
                    renderer.reportUnsupported(node, target_language_1.TargetLanguage.GO);
                    return renderer.convert(base);
                })();
        return new o_tree_1.OTree([
            baseConstructor,
            '_Override(this, ',
            this.argumentList(node.arguments, renderer.updateContext({ wrapPtr: true, isPtr: true })),
            ')',
        ], [], {
            canBreakLine: true,
        });
    }
    methodDeclaration(node, renderer) {
        if (ts.isObjectLiteralExpression(node.parent)) {
            return super.methodDeclaration(node, renderer);
        }
        const className = node.parent.name
            ? this.goName(node.parent.name.text, renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node.parent) }), renderer.typeChecker.getSymbolAtLocation(node.parent.name))
            : 'anonymous';
        const returnType = (0, types_1.determineReturnType)(renderer.typeChecker, node);
        const goReturnType = returnType && this.renderType(node.type ?? node, returnType.symbol, returnType, true, 'interface{}', renderer);
        return new o_tree_1.OTree([
            'func (this *',
            className,
            ') ',
            renderer.updateContext({ isExported: renderer.currentContext.isExported && (0, ast_utils_1.isPublic)(node) }).convert(node.name),
            '(',
            new o_tree_1.OTree([], renderer.convertAll(node.parameters), { separator: ', ' }),
            ') ',
            goReturnType,
            goReturnType ? ' ' : '',
            '{',
        ], [
            this.defaultArgValues(node.parameters, renderer),
            ...(node.body ? renderer.convertAll(node.body.statements) : []),
        ], { canBreakLine: true, suffix: node.body && node.body.statements.length > 0 ? '\n}' : '}', indent: 1 });
    }
    ifStatement(node, renderer) {
        const [ifPrefix, ifSuffix, ifIndent] = ts.isBlock(node.thenStatement) ? [' '] : [' {\n', '\n}', 1];
        const ifStmt = new o_tree_1.OTree(['if ', renderer.convert(node.expression)], [ifPrefix, renderer.convert(node.thenStatement)], {
            canBreakLine: true,
            suffix: ifSuffix,
            indent: ifIndent,
        });
        if (!node.elseStatement) {
            return ifStmt;
        }
        const [elsePrefix, elseSuffix, elseIndent] = ts.isBlock(node.elseStatement) ? [' '] : [' {\n', '\n}', 1];
        const elseStmt = new o_tree_1.OTree(['else'], [elsePrefix, renderer.convert(node.elseStatement)], {
            canBreakLine: true,
            suffix: elseSuffix,
            indent: elseIndent,
        });
        return new o_tree_1.OTree([], [ifStmt, elseStmt], {
            separator: ' ',
            canBreakLine: true,
        });
    }
    forOfStatement(node, renderer) {
        const [prefix, suffix, indent] = ts.isBlock(node.statement) ? [' '] : [' {\n', '\n}', 1];
        return new o_tree_1.OTree(['for _, ', nameOf(node.initializer), ' := range ', renderer.convert(node.expression)], [prefix, renderer.convert(node.statement)], { canBreakLine: true, suffix, indent });
        function nameOf(decl) {
            if (ts.isVariableDeclarationList(decl)) {
                if (decl.declarations.length !== 1) {
                    renderer.reportUnsupported(decl.declarations[1], target_language_1.TargetLanguage.GO);
                }
                return nameOf(decl.declarations[0]);
            }
            if (ts.isVariableDeclaration(decl)) {
                return decl.name.getText(decl.name.getSourceFile());
            }
            renderer.reportUnsupported(decl, target_language_1.TargetLanguage.GO);
            return renderer.convert(decl);
        }
    }
    importStatement(node, renderer) {
        const packageName = node.moduleSymbol?.sourceAssembly?.packageJson.jsii?.targets?.go?.packageName ??
            node.packageName
                // Special case namespaced npm package names, so they are mangled the same way pacmak does.
                .replace(/@([a-z0-9_-]+)\/([a-z0-9_-])/, '$1$2')
                .split('/')
                .map((txt) => this.goName(txt, renderer, undefined))
                .filter((txt) => txt !== '')
                .join('/');
        const moduleName = node.moduleSymbol?.sourceAssembly?.packageJson.jsii?.targets?.go?.moduleName
            ? `${node.moduleSymbol.sourceAssembly.packageJson.jsii.targets.go.moduleName}/${packageName}`
            : `github.com/aws-samples/dummy/${packageName}`;
        if (node.imports.import === 'full') {
            // We don't emit the alias if it matches the last path segment (conventionally this is the package name)
            const maybeAlias = node.imports.alias ? `${this.goName(node.imports.alias, renderer, undefined)} ` : '';
            return new o_tree_1.OTree([`import ${maybeAlias}${JSON.stringify(moduleName)}`], undefined, { canBreakLine: true });
        }
        if (node.imports.elements.length === 0) {
            // This is a blank import (for side-effects only)
            return new o_tree_1.OTree([`import _ ${JSON.stringify(moduleName)}`], undefined, { canBreakLine: true });
        }
        return new o_tree_1.OTree([`import ${JSON.stringify(moduleName)}`], undefined, { canBreakLine: true });
    }
    variableDeclaration(node, renderer) {
        if (!node.initializer) {
            return new o_tree_1.OTree([
                'var ',
                renderer.updateContext({ isExported: (0, ast_utils_1.isExported)(node) }).convert(node.name),
                ' ',
                this.renderTypeNode(node.type, false, renderer) || 'interface{}',
            ]);
        }
        return new o_tree_1.OTree([
            renderer.updateContext({ isExported: false }).convert(node.name),
            ' := ',
            renderer.convert(node.initializer),
        ]);
    }
    defaultArgValues(params, renderer) {
        return new o_tree_1.OTree(params.reduce((accum, param) => {
            if (!param.initializer) {
                return accum;
            }
            const name = renderer.updateContext({ isPtr: true }).convert(param.name);
            return [
                ...accum,
                new o_tree_1.OTree(['\n', 'if ', name, ' == nil {'], ['\n', name, ' = ', renderer.updateContext({ wrapPtr: true }).convert(param.initializer)], {
                    indent: 1,
                    suffix: '\n}',
                }),
            ];
        }, []));
    }
    mergeContext(old, update) {
        return Object.assign({}, old, update);
    }
    renderTypeNode(typeNode, isPtr, renderer) {
        if (!typeNode) {
            return '';
        }
        return this.renderType(typeNode, renderer.typeChecker.getTypeFromTypeNode(typeNode).symbol, renderer.typeOfType(typeNode), isPtr, renderer.textOf(typeNode), renderer);
    }
    renderType(typeNode, typeSymbol, type, isPtr, fallback, renderer) {
        if (type === undefined) {
            return fallback;
        }
        const jsiiType = (0, jsii_types_1.determineJsiiType)(renderer.typeChecker, type);
        const doRender = (jType, asPtr, typeSym) => {
            const prefix = asPtr ? '*' : '';
            switch (jType.kind) {
                case 'unknown':
                    return fallback;
                case 'error':
                    renderer.report(typeNode, jType.message);
                    return fallback;
                case 'map':
                    return `map[string]${doRender(jType.elementType, true, jType.elementTypeSymbol)}`;
                case 'list':
                    return `[]${doRender(jType.elementType, true, jType.elementTypeSymbol)}`;
                case 'namedType':
                    return this.goName(jType.name, renderer, typeSym);
                case 'builtIn':
                    switch (jType.builtIn) {
                        case 'boolean':
                            return `${prefix}bool`;
                        case 'number':
                            return `${prefix}f64`;
                        case 'string':
                            return `${prefix}string`;
                        case 'any':
                            return 'interface{}';
                        case 'void':
                            return '';
                    }
            }
        };
        return doRender(jsiiType, isPtr, typeSymbol);
    }
    /**
     * Guess an item's go name based on it's TS name and context
     */
    goName(input, renderer, symbol) {
        let text = input.replace(/[^a-z0-9_]/gi, '');
        // Symbols can be an index signature, if this is a dot-style access to a map member. In this
        // case we should not cache against the symbol as this would cause all such accesses to the same
        // object to return the same text, which would be incorrect!
        const indexSignature = ts.SymbolFlags.Signature | ts.SymbolFlags.Transient;
        const cacheKey = symbol != null && (symbol.flags & indexSignature) === indexSignature ? input : symbol ?? input;
        const prev = this.idMap.get(cacheKey) ?? this.idMap.get(input);
        if (prev) {
            // If an identifier has been renamed go get it
            text = prev.formatted;
        }
        else if (renderer.currentContext.isExported && !renderer.currentContext.inMapLiteral) {
            // Uppercase exported and public symbols/members
            text = ucFirst(text);
        }
        else if (!renderer.currentContext.inMapLiteral) {
            // Lowercase unexported items that are capitalized in TS like structs/interfaces/classes
            text = lcFirst(text);
        }
        text = prefixReserved(text);
        if (text !== input && prev == null) {
            this.idMap.set(cacheKey, { formatted: text, type: getDeclarationType(renderer.currentContext) });
        }
        if (
        // Non-pointer references to parameters need to be de-referenced
        (!renderer.currentContext.isPtr &&
            !renderer.currentContext.isParameterName &&
            symbol?.valueDeclaration?.kind === ts.SyntaxKind.Parameter &&
            !renderer.currentContext.isPtrAssignmentRValue) ||
            // Pointer reference to non-interfaces are prefixed with *
            (renderer.currentContext.isPtr && prev && prev?.type !== DeclarationType.INTERFACE)) {
            return `*${text}`;
        }
        return text;
    }
}
exports.GoVisitor = GoVisitor;
/**
 * Translation version
 *
 * Bump this when you change something in the implementation to invalidate
 * existing cached translations.
 */
GoVisitor.VERSION = '1';
/**
 * Uppercase the first letter
 */
function ucFirst(x) {
    return x.substring(0, 1).toUpperCase() + x.substring(1);
}
/**
 * Lowercase the first letter
 */
function lcFirst(x) {
    return x.substring(0, 1).toLowerCase() + x.substring(1);
}
function wrapPtrExpression(typeChecker, node, unwrapped) {
    const type = (0, types_1.typeOfExpression)(typeChecker, node);
    const jsiiType = (0, jsii_types_1.determineJsiiType)(typeChecker, type);
    if (jsiiType.kind !== 'builtIn') {
        return unwrapped;
    }
    switch (jsiiType.builtIn) {
        case 'boolean':
            return new o_tree_1.OTree(['jsii.Boolean(', unwrapped, ')']);
        case 'number':
            return new o_tree_1.OTree(['jsii.Number(', unwrapped, ')']);
        case 'string':
            return new o_tree_1.OTree(['jsii.String(', unwrapped, ')']);
        case 'any':
        case 'void':
            return unwrapped;
    }
}
/**
 * Wrap a string literal in the jsii.String helper
 */
function jsiiStr(x) {
    return `jsii.String(${x})`;
}
/**
 * Wrap a string literal in the jsii.String helper
 */
function jsiiNum(x) {
    return `jsii.Number(${x})`;
}
/**
 * Prefix reserved word identifiers with _
 */
function prefixReserved(x) {
    if (['struct'].includes(x)) {
        return `${x}_`;
    }
    return x;
}
function getDeclarationType(ctx) {
    if (ctx.isStruct) {
        return DeclarationType.STRUCT;
    }
    return DeclarationType.UNKNOWN;
}
function findUp(node, predicate) {
    if (predicate(node)) {
        return node;
    }
    if (node.parent == null) {
        return undefined;
    }
    return findUp(node.parent, predicate);
}
function symbolFor(typeChecker, node) {
    if (ts.isIdentifier(node)) {
        return typeChecker.getSymbolAtLocation(node);
    }
    if (ts.isPropertyAccessExpression(node)) {
        return typeChecker.getSymbolAtLocation(node.name);
    }
    // I don't know 🤷🏻‍♂️
    return undefined;
}
/**
 * Checks whether the provided node corresponds to a pointer-value.
 *
 * NOTE: This currently only checkes for parameter declarations. This is
 * presently used only to determine whether a variable reference needs to be
 * wrapped or not (i.e: "jsii.String(varStr)"), and parameter references are the
 * only "always pointer" values possible in that particular context.
 *
 * @param typeChecker a TypeChecker to use to resolve the node's symbol.
 * @param node        the node to be checked.
 *
 * @returns true if the node corresponds to a pointer-value.
 */
function isPointerValue(typeChecker, node) {
    const symbol = typeChecker.getSymbolAtLocation(node);
    if (symbol == null) {
        // Can't find symbol, assuming it's a pointer...
        return true;
    }
    const declaration = symbol.valueDeclaration;
    if (declaration == null) {
        // Doesn't have declaration, assuming it's a pointer...
        return true;
    }
    // Now check if this is known pointer kind or not....
    return ts.isParameter(node);
}
//# sourceMappingURL=go.js.map