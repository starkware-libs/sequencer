"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.AstRenderer = void 0;
exports.nimpl = nimpl;
const ts = require("typescript");
const o_tree_1 = require("./o-tree");
const ast_utils_1 = require("./typescript/ast-utils");
const imports_1 = require("./typescript/imports");
const types_1 = require("./typescript/types");
/**
 * Render a TypeScript AST to some other representation (encoded in OTrees)
 *
 * Dispatch the actual conversion to a specific handler which will get the
 * appropriate method called for particular AST nodes. The handler may use
 * context to modify its own operations when traversing the tree hierarchy,
 * the type of which should be expressed via the C parameter.
 */
class AstRenderer {
    constructor(sourceFile, typeChecker, handler, options = {}, submoduleReferences = new Map()) {
        this.sourceFile = sourceFile;
        this.typeChecker = typeChecker;
        this.handler = handler;
        this.options = options;
        this.submoduleReferences = submoduleReferences;
        this.diagnostics = new Array();
        this.currentContext = handler.defaultContext;
    }
    /**
     * Merge the new context with the current context and create a new Converter from it
     */
    updateContext(contextUpdate) {
        const newContext = this.handler.mergeContext(this.currentContext, contextUpdate);
        // Use prototypal inheritance to create a version of 'this' in which only
        // 'currentContext' is updated.
        return Object.assign(Object.create(this), {
            currentContext: newContext,
        });
    }
    /**
     * Convert a single node to an OTree
     */
    convert(node) {
        if (node === undefined) {
            return o_tree_1.NO_SYNTAX;
        }
        // Basic transform of node
        const transformed = this.dispatch(node);
        transformed.setSpan(node.getStart(this.sourceFile), node.getEnd());
        if (!transformed.attachComment) {
            return transformed;
        }
        const withTrivia = this.attachLeadingTrivia(node, transformed);
        withTrivia.setSpan(node.getStart(this.sourceFile), node.getEnd());
        return withTrivia;
    }
    /**
     * Convert a set of nodes, filtering out hidden nodes
     */
    convertAll(nodes) {
        return filterVisible(nodes).map(this.convert.bind(this));
    }
    convertWithModifier(nodes, makeContext) {
        const vis = assignVisibility(nodes);
        const result = new Array();
        for (const [idx, { node, visible, maskingVoid }] of vis.entries()) {
            const renderedNode = visible ? node : maskingVoid;
            if (renderedNode) {
                const context = makeContext(this, renderedNode, idx);
                result.push(context.convert(renderedNode));
            }
        }
        return result;
    }
    /**
     * Convert a set of nodes, but update the context for the last one.
     *
     * Takes visibility into account.
     */
    convertLastDifferently(nodes, lastContext) {
        const lastConverter = this.updateContext(lastContext);
        const convert = this.convert.bind(this);
        const lastConvert = lastConverter.convert.bind(lastConverter);
        const ret = [];
        const vis = assignVisibility(nodes);
        for (let i = 0; i < vis.length; i++) {
            const whichConvert = i === vis.length - 1 ? lastConvert : convert;
            const node = vis[i].visible ? vis[i].node : vis[i].maskingVoid;
            if (node) {
                ret.push(whichConvert(node));
            }
        }
        return ret;
    }
    getPosition(node) {
        return {
            start: node.getStart(this.sourceFile),
            end: node.getEnd(),
        };
    }
    textOf(node) {
        return node.getText(this.sourceFile);
    }
    textAt(pos, end) {
        return this.sourceFile.text.substring(pos, end);
    }
    /**
     * Infer type of expression by the argument it is assigned to
     *
     * If the type of the expression can include undefined (if the value is
     * optional), `undefined` will be removed from the union.
     *
     * (Will return undefined for object literals not unified with a declared type)
     *
     * @deprecated Use `inferredTypeOfExpression` instead
     */
    inferredTypeOfExpression(node) {
        return (0, types_1.inferredTypeOfExpression)(this.typeChecker, node);
    }
    /**
     * Type of expression from the text of the expression
     *
     * (Will return a map type for object literals)
     *
     * @deprecated Use `typeOfExpression` directly
     */
    typeOfExpression(node) {
        return (0, types_1.typeOfExpression)(this.typeChecker, node);
    }
    typeOfType(node) {
        return this.typeChecker.getTypeFromTypeNode(node);
    }
    typeToString(type) {
        return this.typeChecker.typeToString(type);
    }
    report(node, messageText, category = ts.DiagnosticCategory.Error) {
        this.diagnostics.push({
            category,
            code: 0,
            source: 'rosetta',
            messageText,
            file: this.sourceFile,
            start: node.getStart(this.sourceFile),
            length: node.getWidth(this.sourceFile),
        });
    }
    reportUnsupported(node, language) {
        const nodeKind = ts.SyntaxKind[node.kind];
        // tslint:disable-next-line:max-line-length
        if (language) {
            this.report(node, `This TypeScript feature (${nodeKind}) is not supported in examples because we cannot translate it to ${language}. Please rewrite this example.`);
        }
        else {
            this.report(node, `This TypeScript feature (${nodeKind}) is not supported in examples. Please rewrite this example.`);
        }
    }
    /**
     * Whether there is non-whitespace on the same line before the given position
     */
    codeOnLineBefore(pos) {
        const text = this.sourceFile.text;
        while (pos > 0) {
            const c = text[--pos];
            if (c === '\n') {
                return false;
            }
            if (c !== ' ' && c !== '\r' && c !== '\t') {
                return true;
            }
        }
        return false;
    }
    /**
     * Return a newline if the given node is preceded by at least one newline
     *
     * Used to mirror newline use between matchin brackets (such as { ... } and [ ... ]).
     */
    mirrorNewlineBefore(viz, suffix = '', otherwise = '') {
        if (viz === undefined) {
            return suffix;
        }
        // Return a newline if the given node is preceded by newlines
        const leadingRanges = (0, ast_utils_1.scanText)(this.sourceFile.text, viz.getFullStart(), viz.getStart(this.sourceFile));
        const newlines = [];
        for (const range of leadingRanges) {
            if (range.type === 'other') {
                newlines.push((0, ast_utils_1.repeatNewlines)(this.sourceFile.text.substring(range.pos, range.end)));
            }
        }
        return (newlines.join('').length > 0 ? '\n' : otherwise) + suffix;
    }
    /**
     * Dispatch node to handler
     */
    dispatch(tree) {
        const visitor = this.handler;
        // Using a switch on tree.kind + forced down-casting, because this is significantly faster than
        // doing a cascade of `if` statements with the `ts.is<NodeType>` functions, since `tree.kind` is
        // effectively integers, and this switch statement is hence optimizable to a jump table. This is
        // a VERY significant enhancement to the debugging experience, too.
        switch (tree.kind) {
            case ts.SyntaxKind.EmptyStatement:
                // Additional semicolon where it doesn't belong.
                return o_tree_1.NO_SYNTAX;
            case ts.SyntaxKind.SourceFile:
                return visitor.sourceFile(tree, this);
            case ts.SyntaxKind.ImportEqualsDeclaration:
                return visitor.importStatement((0, imports_1.analyzeImportEquals)(tree, this), this);
            case ts.SyntaxKind.ImportDeclaration:
                return new o_tree_1.OTree([], (0, imports_1.analyzeImportDeclaration)(tree, this, this.submoduleReferences).map((import_) => visitor.importStatement(import_, this)), { canBreakLine: true, separator: '\n' });
            case ts.SyntaxKind.StringLiteral:
            case ts.SyntaxKind.NoSubstitutionTemplateLiteral:
                return visitor.stringLiteral(tree, this);
            case ts.SyntaxKind.NumericLiteral:
                return visitor.numericLiteral(tree, this);
            case ts.SyntaxKind.FunctionDeclaration:
                return visitor.functionDeclaration(tree, this);
            case ts.SyntaxKind.Identifier:
                return visitor.identifier(tree, this);
            case ts.SyntaxKind.Block:
                return visitor.block(tree, this);
            case ts.SyntaxKind.Parameter:
                return visitor.parameterDeclaration(tree, this);
            case ts.SyntaxKind.ReturnStatement:
                return visitor.returnStatement(tree, this);
            case ts.SyntaxKind.BinaryExpression:
                return visitor.binaryExpression(tree, this);
            case ts.SyntaxKind.IfStatement:
                return visitor.ifStatement(tree, this);
            case ts.SyntaxKind.PropertyAccessExpression:
                const submoduleReference = this.submoduleReferences?.get(tree);
                return visitor.propertyAccessExpression(tree, this, submoduleReference);
            case ts.SyntaxKind.AwaitExpression:
                return visitor.awaitExpression(tree, this);
            case ts.SyntaxKind.CallExpression:
                return visitor.callExpression(tree, this);
            case ts.SyntaxKind.ExpressionStatement:
                return visitor.expressionStatement(tree, this);
            case ts.SyntaxKind.ObjectLiteralExpression:
                return visitor.objectLiteralExpression(tree, this);
            case ts.SyntaxKind.NewExpression:
                return visitor.newExpression(tree, this);
            case ts.SyntaxKind.PropertyAssignment:
                return visitor.propertyAssignment(tree, this);
            case ts.SyntaxKind.VariableStatement:
                return visitor.variableStatement(tree, this);
            case ts.SyntaxKind.VariableDeclarationList:
                return visitor.variableDeclarationList(tree, this);
            case ts.SyntaxKind.VariableDeclaration:
                return visitor.variableDeclaration(tree, this);
            case ts.SyntaxKind.ArrayLiteralExpression:
                return visitor.arrayLiteralExpression(tree, this);
            case ts.SyntaxKind.ShorthandPropertyAssignment:
                return visitor.shorthandPropertyAssignment(tree, this);
            case ts.SyntaxKind.ForOfStatement:
                return visitor.forOfStatement(tree, this);
            case ts.SyntaxKind.ClassDeclaration:
                return visitor.classDeclaration(tree, this);
            case ts.SyntaxKind.Constructor:
                return visitor.constructorDeclaration(tree, this);
            case ts.SyntaxKind.PropertyDeclaration:
                return visitor.propertyDeclaration(tree, this);
            case ts.SyntaxKind.ComputedPropertyName:
                return visitor.computedPropertyName(tree.expression, this);
            case ts.SyntaxKind.MethodDeclaration:
                return visitor.methodDeclaration(tree, this);
            case ts.SyntaxKind.InterfaceDeclaration:
                return visitor.interfaceDeclaration(tree, this);
            case ts.SyntaxKind.PropertySignature:
                return visitor.propertySignature(tree, this);
            case ts.SyntaxKind.MethodSignature:
                return visitor.methodSignature(tree, this);
            case ts.SyntaxKind.AsExpression:
                return visitor.asExpression(tree, this);
            case ts.SyntaxKind.PrefixUnaryExpression:
                return visitor.prefixUnaryExpression(tree, this);
            case ts.SyntaxKind.SpreadAssignment:
                if (this.textOf(tree) === '...') {
                    return visitor.ellipsis(tree, this);
                }
                return visitor.spreadAssignment(tree, this);
            case ts.SyntaxKind.SpreadElement:
                if (this.textOf(tree) === '...') {
                    return visitor.ellipsis(tree, this);
                }
                return visitor.spreadElement(tree, this);
            case ts.SyntaxKind.ElementAccessExpression:
                return visitor.elementAccessExpression(tree, this);
            case ts.SyntaxKind.TemplateExpression:
                return visitor.templateExpression(tree, this);
            case ts.SyntaxKind.NonNullExpression:
                return visitor.nonNullExpression(tree, this);
            case ts.SyntaxKind.ParenthesizedExpression:
                return visitor.parenthesizedExpression(tree, this);
            case ts.SyntaxKind.VoidExpression:
                return visitor.maskingVoidExpression(tree, this);
            case ts.SyntaxKind.JSDoc:
            case ts.SyntaxKind.JSDocComment:
                return visitor.jsDoc(tree, this);
            default:
                if (ts.isToken(tree)) {
                    return visitor.token(tree, this);
                }
                this.reportUnsupported(tree, undefined);
        }
        if (this.options.bestEffort !== false) {
            // When doing best-effort conversion and we don't understand the node type, just return the complete text of it as-is
            return new o_tree_1.OTree([this.textOf(tree)]);
        }
        // Otherwise, show a placeholder indicating we don't recognize the type
        const nodeKind = ts.SyntaxKind[tree.kind];
        return new o_tree_1.UnknownSyntax([`<${nodeKind} ${this.textOf(tree)}>`], ['\n', ...(0, ast_utils_1.nodeChildren)(tree).map(this.convert.bind(this))], {
            indent: 2,
        });
    }
    /**
     * Attach any leading whitespace and comments to the given output tree
     *
     * Regardless of whether it's declared to be able to accept such or not.
     */
    attachLeadingTrivia(node, transformed) {
        // Add comments and leading whitespace
        const leadingRanges = (0, ast_utils_1.scanText)(this.sourceFile.text, node.getFullStart(), node.getStart(this.sourceFile));
        const precede = [];
        for (const range of leadingRanges) {
            let trivia = undefined;
            switch (range.type) {
                case 'other':
                    trivia = new o_tree_1.OTree([(0, ast_utils_1.repeatNewlines)(this.sourceFile.text.substring(range.pos, range.end))], [], {
                        renderOnce: `ws-${range.pos}`,
                    });
                    break;
                case 'linecomment':
                case 'blockcomment':
                    trivia = this.handler.commentRange(commentSyntaxFromCommentRange((0, ast_utils_1.commentRangeFromTextRange)(range), this), this);
                    break;
                case 'directive':
                    break;
            }
            if (trivia != null) {
                // Set spans on comments to make sure their visibility is toggled correctly.
                trivia.setSpan(range.pos, range.end);
                precede.push(trivia);
            }
        }
        // FIXME: No trailing comments for now, they're too tricky
        if (precede.length > 0 && !transformed.isEmpty) {
            return new o_tree_1.OTree([...precede, transformed], [], { canBreakLine: true });
        }
        return transformed;
    }
}
exports.AstRenderer = AstRenderer;
function nimpl(node, context, options = {}) {
    const children = (0, ast_utils_1.nodeChildren)(node).map((c) => context.convert(c));
    let syntaxKind = ts.SyntaxKind[node.kind];
    if (syntaxKind === 'FirstPunctuation') {
        // These have the same identifier but this name is more descriptive
        syntaxKind = 'OpenBraceToken';
    }
    const parts = [`(${syntaxKind}`];
    if (options.additionalInfo) {
        parts.push(`{${options.additionalInfo}}`);
    }
    parts.push(context.textOf(node));
    return new o_tree_1.UnknownSyntax([parts.join(' ')], children.length > 0 ? ['\n', ...children] : [], {
        indent: 2,
        suffix: ')',
        separator: '\n',
        canBreakLine: true,
    });
}
function filterVisible(nodes) {
    return assignVisibility(nodes)
        .map((c) => (c.visible ? c.node : c.maskingVoid))
        .filter(notUndefined);
}
function assignVisibility(nodes) {
    const ret = [];
    let visible = true;
    for (const node of nodes) {
        const maskingVoid = (0, ast_utils_1.extractMaskingVoidExpression)(node);
        if (visible && maskingVoid) {
            visible = false;
        }
        ret.push({ node, maskingVoid, visible });
        if (!visible) {
            const showing = (0, ast_utils_1.extractShowingVoidExpression)(node);
            if (showing) {
                visible = true;
            }
        }
    }
    return ret;
}
function notUndefined(x) {
    return x !== undefined;
}
function commentSyntaxFromCommentRange(rng, renderer) {
    return {
        hasTrailingNewLine: rng.hasTrailingNewLine,
        kind: rng.kind,
        pos: rng.pos,
        text: renderer.textAt(rng.pos, rng.end),
        isTrailing: renderer.codeOnLineBefore(rng.pos),
    };
}
//# sourceMappingURL=renderer.js.map