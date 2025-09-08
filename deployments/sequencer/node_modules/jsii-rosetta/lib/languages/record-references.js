"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.RecordReferencesVisitor = void 0;
const default_1 = require("./default");
const record_references_version_1 = require("./record-references-version");
const jsii_utils_1 = require("../jsii/jsii-utils");
const target_language_1 = require("../languages/target-language");
const o_tree_1 = require("../o-tree");
/**
 * A visitor that collects all types referenced in a particular piece of sample code
 */
class RecordReferencesVisitor extends default_1.DefaultVisitor {
    constructor(visibleSpans) {
        super();
        this.visibleSpans = visibleSpans;
        this.language = target_language_1.TargetLanguage.VISUALIZE;
        this.defaultContext = {};
        this.references = new Set();
    }
    fqnsReferenced() {
        return Array.from(this.references).sort();
    }
    mergeContext(old, update) {
        return Object.assign({}, old, update);
    }
    /**
     * For a variable declaration, a type counts as "referenced" if it gets assigned a value via an initializer
     *
     * This skips "declare" statements which aren't really interesting.
     */
    variableDeclaration(node, renderer) {
        if (this.visibleSpans.containsStartOfNode(node) && node.initializer) {
            const type = (node.type && renderer.typeOfType(node.type)) ||
                (node.initializer && renderer.typeOfExpression(node.initializer));
            this.recordSymbol(type.symbol, renderer);
        }
        return super.variableDeclaration(node, renderer);
    }
    newExpression(node, context) {
        // Constructor
        if (this.visibleSpans.containsStartOfNode(node)) {
            this.recordNode(node.expression, context);
            if (node.arguments) {
                this.visitArgumentTypes(node.arguments, context);
            }
        }
        return super.newExpression(node, context);
    }
    propertyAccessExpression(node, context, submoduleReference) {
        if (this.visibleSpans.containsStartOfNode(node)) {
            // The property itself
            this.recordNode(node, context);
            // Not currently considering the return type as "referenced"
        }
        return super.propertyAccessExpression(node, context, submoduleReference);
    }
    regularCallExpression(node, context) {
        if (this.visibleSpans.containsStartOfNode(node)) {
            // The method itself
            this.recordNode(node.expression, context);
            if (node.arguments) {
                this.visitArgumentTypes(node.arguments, context);
            }
            // Not currently considering the return type as "referenced"
        }
        return super.regularCallExpression(node, context);
    }
    objectLiteralExpression(node, context) {
        context.convertAll(node.properties);
        return o_tree_1.NO_SYNTAX;
    }
    propertyAssignment(node, renderer) {
        const type = renderer.typeOfExpression(node.initializer).getNonNullableType();
        this.recordSymbol(type?.symbol, renderer);
        return super.propertyAssignment(node, renderer);
    }
    shorthandPropertyAssignment(node, renderer) {
        const type = renderer.typeOfExpression(node.name).getNonNullableType();
        this.recordSymbol(type?.symbol, renderer);
        return super.shorthandPropertyAssignment(node, renderer);
    }
    /**
     * Visit the arguments by type (instead of by node)
     *
     * This will make sure we recognize the use of a `BucketProps` in a `new Bucket(..., { ... })` call.
     */
    visitArgumentTypes(args, context) {
        for (const argument of args) {
            const type = context.inferredTypeOfExpression(argument);
            this.recordSymbol(type?.symbol, context);
        }
    }
    recordNode(node, context) {
        this.recordSymbol(context.typeChecker.getSymbolAtLocation(node), context);
    }
    recordSymbol(symbol, context) {
        if (!symbol) {
            return;
        }
        const jsiiSym = (0, jsii_utils_1.lookupJsiiSymbol)(context.typeChecker, symbol);
        if (!jsiiSym) {
            return;
        }
        this.references.add(jsiiSym.fqn);
    }
}
exports.RecordReferencesVisitor = RecordReferencesVisitor;
RecordReferencesVisitor.VERSION = record_references_version_1.RECORD_REFERENCES_VERSION;
//# sourceMappingURL=record-references.js.map