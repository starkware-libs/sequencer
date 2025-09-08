"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DefaultVisitor = void 0;
const ts = require("typescript");
const jsii_types_1 = require("../jsii/jsii-types");
const jsii_utils_1 = require("../jsii/jsii-utils");
const o_tree_1 = require("../o-tree");
const renderer_1 = require("../renderer");
const ast_utils_1 = require("../typescript/ast-utils");
const types_1 = require("../typescript/types");
/**
 * A basic visitor that applies for most curly-braces-based languages
 */
class DefaultVisitor {
    constructor() {
        this.statementTerminator = ';';
    }
    commentRange(comment, _context) {
        return new o_tree_1.OTree([comment.isTrailing ? ' ' : '', comment.text, comment.hasTrailingNewLine ? '\n' : '']);
    }
    sourceFile(node, context) {
        return new o_tree_1.OTree(context.convertAll(node.statements));
    }
    jsDoc(_node, _context) {
        // Already handled by other doc handlers
        return new o_tree_1.OTree([]);
    }
    importStatement(node, context) {
        return this.notImplemented(node.node, context);
    }
    functionDeclaration(node, children) {
        return this.notImplemented(node, children);
    }
    stringLiteral(node, _renderer) {
        return new o_tree_1.OTree([JSON.stringify(node.text)]);
    }
    numericLiteral(node, _children) {
        return new o_tree_1.OTree([node.text]);
    }
    identifier(node, _children) {
        return new o_tree_1.OTree([node.text]);
    }
    block(node, children) {
        return new o_tree_1.OTree(['{'], ['\n', ...children.convertAll(node.statements)], {
            indent: 4,
            suffix: '}',
        });
    }
    parameterDeclaration(node, children) {
        return this.notImplemented(node, children);
    }
    returnStatement(node, children) {
        return new o_tree_1.OTree(['return ', children.convert(node.expression), this.statementTerminator], [], {
            canBreakLine: true,
        });
    }
    binaryExpression(node, context) {
        const operator = context.textOf(node.operatorToken);
        if (operator === '??') {
            context.reportUnsupported(node.operatorToken, undefined);
        }
        const operatorToken = this.translateBinaryOperator(operator);
        return new o_tree_1.OTree([context.convert(node.left), ' ', operatorToken, ' ', context.convert(node.right)]);
    }
    prefixUnaryExpression(node, context) {
        return new o_tree_1.OTree([this.translateUnaryOperator(node.operator), context.convert(node.operand)]);
    }
    translateUnaryOperator(operator) {
        return UNARY_OPS[operator];
    }
    translateBinaryOperator(operator) {
        if (operator === '===') {
            return '==';
        }
        return operator;
    }
    ifStatement(node, context) {
        return this.notImplemented(node, context);
    }
    propertyAccessExpression(node, context, _submoduleReference) {
        return new o_tree_1.OTree([context.convert(node.expression), '.', context.convert(node.name)]);
    }
    /**
     * Do some work on property accesses to translate common JavaScript-isms to language-specific idioms
     */
    callExpression(node, context) {
        const functionText = context.textOf(node.expression);
        if (functionText === 'console.log' || functionText === 'console.error') {
            return this.printStatement(node.arguments, context);
        }
        if (functionText === 'super') {
            return this.superCallExpression(node, context);
        }
        return this.regularCallExpression(node, context);
    }
    awaitExpression(node, context) {
        return context.convert(node.expression);
    }
    regularCallExpression(node, context) {
        return new o_tree_1.OTree([context.convert(node.expression), '(', this.argumentList(node.arguments, context), ')']);
    }
    superCallExpression(node, context) {
        return this.regularCallExpression(node, context);
    }
    printStatement(args, context) {
        return new o_tree_1.OTree(['<PRINT>', '(', this.argumentList(args, context), ')']);
    }
    expressionStatement(node, context) {
        return new o_tree_1.OTree([context.convert(node.expression)], [], {
            canBreakLine: true,
        });
    }
    token(node, context) {
        return new o_tree_1.OTree([context.textOf(node)]);
    }
    /**
     * An object literal can render as one of three things:
     *
     * - Don't know the type (render as an unknown struct)
     * - Know the type:
     *     - It's a struct (render as known struct)
     *     - It's not a struct (render as key-value map)
     */
    objectLiteralExpression(node, context) {
        // If any of the elements of the objectLiteralExpression are not a literal property
        // assignment, report them. We can't support those.
        const unsupported = node.properties.filter((p) => !ts.isPropertyAssignment(p) && !ts.isShorthandPropertyAssignment(p));
        for (const unsup of unsupported) {
            context.report(unsup, `Use of ${ts.SyntaxKind[unsup.kind]} in an object literal is not supported.`);
        }
        const anyMembersFunctions = node.properties.some((p) => ts.isPropertyAssignment(p)
            ? isExpressionOfFunctionType(context.typeChecker, p.initializer)
            : ts.isShorthandPropertyAssignment(p)
                ? isExpressionOfFunctionType(context.typeChecker, p.name)
                : false);
        const inferredType = (0, types_1.inferredTypeOfExpression)(context.typeChecker, node);
        if ((inferredType && (0, jsii_utils_1.isJsiiProtocolType)(context.typeChecker, inferredType)) || anyMembersFunctions) {
            context.report(node, `You cannot use an object literal to make an instance of an interface. Define a class instead.`);
        }
        const lit = (0, jsii_types_1.analyzeObjectLiteral)(context.typeChecker, node);
        switch (lit.kind) {
            case 'unknown':
                return this.unknownTypeObjectLiteralExpression(node, context);
            case 'struct':
            case 'local-struct':
                return this.knownStructObjectLiteralExpression(node, lit, context);
            case 'map':
                return this.keyValueObjectLiteralExpression(node, context);
        }
    }
    unknownTypeObjectLiteralExpression(node, context) {
        return this.notImplemented(node, context);
    }
    knownStructObjectLiteralExpression(node, _structType, context) {
        return this.notImplemented(node, context);
    }
    keyValueObjectLiteralExpression(node, context) {
        return this.notImplemented(node, context);
    }
    newExpression(node, context) {
        return new o_tree_1.OTree(['new ', context.convert(node.expression), '(', this.argumentList(node.arguments, context), ')'], [], { canBreakLine: true });
    }
    propertyAssignment(node, context) {
        return this.notImplemented(node, context);
    }
    variableStatement(node, context) {
        return new o_tree_1.OTree([context.convert(node.declarationList)], [], {
            canBreakLine: true,
        });
    }
    variableDeclarationList(node, context) {
        return new o_tree_1.OTree([], context.convertAll(node.declarations));
    }
    variableDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    arrayLiteralExpression(node, context) {
        return new o_tree_1.OTree(['['], context.convertAll(node.elements), {
            separator: ', ',
            suffix: ']',
        });
    }
    shorthandPropertyAssignment(node, context) {
        return this.notImplemented(node, context);
    }
    forOfStatement(node, context) {
        return this.notImplemented(node, context);
    }
    classDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    constructorDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    propertyDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    computedPropertyName(node, context) {
        return context.convert(node);
    }
    methodDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    interfaceDeclaration(node, context) {
        if ((0, jsii_utils_1.isNamedLikeStruct)(context.textOf(node.name))) {
            return this.structInterfaceDeclaration(node, context);
        }
        return this.regularInterfaceDeclaration(node, context);
    }
    structInterfaceDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    regularInterfaceDeclaration(node, context) {
        return this.notImplemented(node, context);
    }
    propertySignature(node, context) {
        return this.notImplemented(node, context);
    }
    methodSignature(node, context) {
        return this.notImplemented(node, context);
    }
    asExpression(node, context) {
        return this.notImplemented(node, context);
    }
    spreadElement(node, context) {
        return this.notImplemented(node, context);
    }
    spreadAssignment(node, context) {
        return this.notImplemented(node, context);
    }
    ellipsis(_node, _context) {
        return new o_tree_1.OTree(['...']);
    }
    templateExpression(node, context) {
        return this.notImplemented(node, context);
    }
    elementAccessExpression(node, context) {
        const expression = context.convert(node.expression);
        const index = context.convert(node.argumentExpression);
        return new o_tree_1.OTree([expression, '[', index, ']']);
    }
    nonNullExpression(node, context) {
        // We default we drop the non-null assertion
        return context.convert(node.expression);
    }
    parenthesizedExpression(node, context) {
        return new o_tree_1.OTree(['(', context.convert(node.expression), ')']);
    }
    maskingVoidExpression(node, context) {
        // Don't render anything by default when nodes are masked
        const arg = (0, ast_utils_1.voidExpressionString)(node);
        if (arg === 'block') {
            return this.commentRange({
                pos: context.getPosition(node).start,
                text: '\n// ...',
                kind: ts.SyntaxKind.SingleLineCommentTrivia,
                hasTrailingNewLine: false,
            }, context);
        }
        if (arg === '...') {
            return new o_tree_1.OTree(['...']);
        }
        return o_tree_1.NO_SYNTAX;
    }
    argumentList(args, context) {
        return new o_tree_1.OTree([], args ? context.convertAll(args) : [], {
            separator: ', ',
        });
    }
    notImplemented(node, context) {
        context.reportUnsupported(node, this.language);
        return (0, renderer_1.nimpl)(node, context);
    }
}
exports.DefaultVisitor = DefaultVisitor;
const UNARY_OPS = {
    [ts.SyntaxKind.PlusPlusToken]: '++',
    [ts.SyntaxKind.MinusMinusToken]: '--',
    [ts.SyntaxKind.PlusToken]: '+',
    [ts.SyntaxKind.MinusToken]: '-',
    [ts.SyntaxKind.TildeToken]: '~',
    [ts.SyntaxKind.ExclamationToken]: '!',
};
/**
 * Whether the given expression evaluates to a value that is of type "function"
 *
 * Examples of function types:
 *
 * ```ts
 * // GIVEN
 * function someFunction() { }
 *
 * // THEN
 * const x = someFunction; // <- function type
 * const y = () => 42; // <- function type
 * const z = x; // <- function type
 * Array.isArray; // <- function type
 * ```
 */
function isExpressionOfFunctionType(typeChecker, expr) {
    const type = typeChecker.getTypeAtLocation(expr).getNonNullableType();
    return type.getCallSignatures().length > 0;
}
//# sourceMappingURL=default.js.map