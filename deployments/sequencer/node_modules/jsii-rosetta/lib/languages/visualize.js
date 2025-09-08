"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VisualizeAstVisitor = void 0;
const target_language_1 = require("./target-language");
const o_tree_1 = require("../o-tree");
const renderer_1 = require("../renderer");
class VisualizeAstVisitor {
    constructor(includeHandlerNames) {
        this.includeHandlerNames = includeHandlerNames;
        this.language = target_language_1.TargetLanguage.VISUALIZE;
        this.defaultContext = undefined;
    }
    mergeContext(_old, _update) {
        return undefined;
    }
    commentRange(node, _context) {
        return new o_tree_1.OTree(['(Comment', node.text], [], { suffix: ')' });
    }
    jsDoc(_node, _context) {
        // Already handled by other doc handlers
        return new o_tree_1.OTree([]);
    }
    sourceFile(node, context) {
        return new o_tree_1.OTree(context.convertAll(node.statements));
    }
    importStatement(node, context) {
        return this.defaultNode('importStatement', node.node, context);
    }
    functionDeclaration(node, children) {
        return this.defaultNode('functionDeclaration', node, children);
    }
    stringLiteral(node, children) {
        return this.defaultNode('stringLiteral', node, children);
    }
    numericLiteral(node, children) {
        return this.defaultNode('numericLiteral', node, children);
    }
    identifier(node, children) {
        return this.defaultNode('identifier', node, children);
    }
    block(node, children) {
        return this.defaultNode('block', node, children);
    }
    parameterDeclaration(node, children) {
        return this.defaultNode('parameterDeclaration', node, children);
    }
    returnStatement(node, children) {
        return this.defaultNode('returnStatement', node, children);
    }
    binaryExpression(node, children) {
        return this.defaultNode('binaryExpression', node, children);
    }
    ifStatement(node, context) {
        return this.defaultNode('ifStatement', node, context);
    }
    propertyAccessExpression(node, context) {
        return this.defaultNode('propertyAccessExpression', node, context);
    }
    callExpression(node, context) {
        return this.defaultNode('callExpression', node, context);
    }
    expressionStatement(node, context) {
        return this.defaultNode('expressionStatement', node, context);
    }
    token(node, context) {
        return this.defaultNode('token', node, context);
    }
    objectLiteralExpression(node, context) {
        return this.defaultNode('objectLiteralExpression', node, context);
    }
    newExpression(node, context) {
        return this.defaultNode('newExpression', node, context);
    }
    awaitExpression(node, context) {
        return this.defaultNode('await', node, context);
    }
    propertyAssignment(node, context) {
        return this.defaultNode('propertyAssignment', node, context);
    }
    variableStatement(node, context) {
        return this.defaultNode('variableStatement', node, context);
    }
    variableDeclarationList(node, context) {
        return this.defaultNode('variableDeclarationList', node, context);
    }
    variableDeclaration(node, context) {
        return this.defaultNode('variableDeclaration', node, context);
    }
    arrayLiteralExpression(node, context) {
        return this.defaultNode('arrayLiteralExpression', node, context);
    }
    shorthandPropertyAssignment(node, context) {
        return this.defaultNode('shorthandPropertyAssignment', node, context);
    }
    forOfStatement(node, context) {
        return this.defaultNode('forOfStatement', node, context);
    }
    classDeclaration(node, context) {
        return this.defaultNode('classDeclaration', node, context);
    }
    constructorDeclaration(node, context) {
        return this.defaultNode('constructorDeclaration', node, context);
    }
    propertyDeclaration(node, context) {
        return this.defaultNode('propertyDeclaration', node, context);
    }
    computedPropertyName(node, context) {
        return this.defaultNode('computedPropertyName', node, context);
    }
    methodDeclaration(node, context) {
        return this.defaultNode('methodDeclaration', node, context);
    }
    interfaceDeclaration(node, context) {
        return this.defaultNode('interfaceDeclaration', node, context);
    }
    propertySignature(node, context) {
        return this.defaultNode('propertySignature', node, context);
    }
    methodSignature(node, context) {
        return this.defaultNode('methodSignature', node, context);
    }
    asExpression(node, context) {
        return this.defaultNode('asExpression', node, context);
    }
    prefixUnaryExpression(node, context) {
        return this.defaultNode('prefixUnaryExpression', node, context);
    }
    spreadElement(node, context) {
        return this.defaultNode('spreadElement', node, context);
    }
    spreadAssignment(node, context) {
        return this.defaultNode('spreadAssignment', node, context);
    }
    ellipsis(node, context) {
        return this.defaultNode('ellipsis', node, context);
    }
    templateExpression(node, context) {
        return this.defaultNode('templateExpression', node, context);
    }
    elementAccessExpression(node, context) {
        return this.defaultNode('elementAccessExpression', node, context);
    }
    nonNullExpression(node, context) {
        return this.defaultNode('nonNullExpression', node, context);
    }
    parenthesizedExpression(node, context) {
        return this.defaultNode('parenthesizedExpression', node, context);
    }
    maskingVoidExpression(node, context) {
        return this.defaultNode('maskingVoidExpression', node, context);
    }
    defaultNode(handlerName, node, context) {
        return (0, renderer_1.nimpl)(node, context, {
            additionalInfo: this.includeHandlerNames ? handlerName : '',
        });
    }
}
exports.VisualizeAstVisitor = VisualizeAstVisitor;
//# sourceMappingURL=visualize.js.map