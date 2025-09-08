"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.isStatic = exports.isProtected = exports.isPrivate = exports.isExported = exports.isReadOnly = exports.DONE = void 0;
exports.stripCommentMarkers = stripCommentMarkers;
exports.stringFromLiteral = stringFromLiteral;
exports.nodeChildren = nodeChildren;
exports.nodeOfType = nodeOfType;
exports.anyNode = anyNode;
exports.allOfType = allOfType;
exports.matchAst = matchAst;
exports.countNakedNewlines = countNakedNewlines;
exports.repeatNewlines = repeatNewlines;
exports.extractComments = extractComments;
exports.commentRangeFromTextRange = commentRangeFromTextRange;
exports.scanText = scanText;
exports.extractMaskingVoidExpression = extractMaskingVoidExpression;
exports.extractShowingVoidExpression = extractShowingVoidExpression;
exports.voidExpressionString = voidExpressionString;
exports.extractVoidExpression = extractVoidExpression;
exports.quoteStringLiteral = quoteStringLiteral;
exports.visibility = visibility;
exports.isPublic = isPublic;
exports.findSuperCall = findSuperCall;
exports.privatePropertyNames = privatePropertyNames;
exports.findEnclosingClassDeclaration = findEnclosingClassDeclaration;
const ts = require("typescript");
function stripCommentMarkers(comment, multiline) {
    if (multiline) {
        // The text *must* start with '/*' and end with '*/'.
        // Strip leading '*' from every remaining line (first line because of '**',
        // other lines because of continuations.
        return comment
            .substring(2, comment.length - 2)
            .replace(/^[ \t]+/g, '') // Strip all leading whitepace
            .replace(/[ \t]+$/g, '') // Strip all trailing whitepace
            .replace(/^[ \t]*\*[ \t]?/gm, ''); // Strip "* " from start of line
    }
    // The text *must* start with '//'
    return comment.replace(/^[/]{2}[ \t]?/gm, '');
}
function stringFromLiteral(expr) {
    if (ts.isStringLiteral(expr)) {
        return expr.text;
    }
    return '???';
}
/**
 * Return AST children of the given node
 *
 * Difference with node.getChildren():
 *
 * - node.getChildren() must take a SourceFile (will fail if it doesn't get it)
 *   and returns a mix of abstract and concrete syntax nodes.
 * - This function function will ONLY return abstract syntax nodes.
 */
function nodeChildren(node) {
    const ret = new Array();
    node.forEachChild((n) => {
        ret.push(n);
    });
    return ret;
}
// eslint-disable-next-line max-len
function nodeOfType(syntaxKindOrCaptureName, nodeTypeOrChildren, children) {
    const capturing = typeof syntaxKindOrCaptureName === 'string'; // Determine which overload we're in (SyntaxKind is a number)
    const realNext = (capturing ? children : nodeTypeOrChildren) ?? exports.DONE;
    const realCapture = capturing ? syntaxKindOrCaptureName : undefined;
    const realSyntaxKind = capturing ? nodeTypeOrChildren : syntaxKindOrCaptureName;
    return (nodes) => {
        for (const node of nodes ?? []) {
            if (node.kind === realSyntaxKind) {
                const ret = realNext(nodeChildren(node));
                if (!ret) {
                    continue;
                }
                if (realCapture) {
                    return Object.assign(ret, {
                        [realCapture]: node,
                    });
                }
                return ret;
            }
        }
        return undefined;
    };
}
function anyNode(children) {
    const realNext = children ?? exports.DONE;
    return (nodes) => {
        for (const node of nodes ?? []) {
            const m = realNext(nodeChildren(node));
            if (m) {
                return m;
            }
        }
        return undefined;
    };
}
// Does not capture deeper because how would we even represent that?
function allOfType(s, name, children) {
    const realNext = children ?? exports.DONE;
    return (nodes) => {
        let ret;
        for (const node of nodes ?? []) {
            if (node.kind === s) {
                if (realNext(nodeChildren(node))) {
                    if (!ret) {
                        ret = { [name]: new Array() };
                    }
                    ret[name].push(node);
                }
            }
        }
        return ret;
    };
}
const DONE = () => ({});
exports.DONE = DONE;
function matchAst(node, matcher, cb) {
    const matched = matcher([node]);
    if (cb) {
        if (matched) {
            cb(matched);
        }
        return !!matched;
    }
    return matched;
}
/**
 * Count the newlines in a given piece of string that aren't in comment blocks
 */
function countNakedNewlines(str) {
    let ret = 0;
    for (const s of scanText(str, 0, str.length).filter((r) => r.type === 'other' || r.type === 'blockcomment')) {
        if (s.type === 'other') {
            // Count newlines in non-comments
            for (let i = s.pos; i < s.end; i++) {
                if (str[i] === '\n') {
                    ret++;
                }
            }
        }
        else {
            // Discount newlines at the end of block comments
            if (s.hasTrailingNewLine) {
                ret--;
            }
        }
    }
    return ret;
}
function repeatNewlines(str) {
    return '\n'.repeat(Math.min(2, countNakedNewlines(str)));
}
const WHITESPACE = [' ', '\t', '\r', '\n'];
/**
 * Extract single-line and multi-line comments from the given string
 *
 * Rewritten because I can't get ts.getLeadingComments and ts.getTrailingComments to do what I want.
 */
function extractComments(text, start) {
    return scanText(text, start)
        .filter((s) => s.type === 'blockcomment' || s.type === 'linecomment')
        .map(commentRangeFromTextRange);
}
function commentRangeFromTextRange(rng) {
    return {
        kind: rng.type === 'blockcomment' ? ts.SyntaxKind.MultiLineCommentTrivia : ts.SyntaxKind.SingleLineCommentTrivia,
        pos: rng.pos,
        end: rng.end,
        hasTrailingNewLine: rng.type !== 'blockcomment' && rng.hasTrailingNewLine,
    };
}
/**
 * Extract spans of comments and non-comments out of the string
 *
 * Stop at 'end' when given, or the first non-whitespace character in a
 * non-comment if not given.
 */
function scanText(text, start, end) {
    const ret = [];
    let pos = start;
    const stopAtCode = end === undefined;
    if (end === undefined) {
        end = text.length;
    }
    while (pos < end) {
        const ch = text[pos];
        if (WHITESPACE.includes(ch)) {
            pos++;
            continue;
        }
        if (ch === '/' && text[pos + 1] === '/') {
            accumulateTextBlock();
            scanSinglelineComment();
            continue;
        }
        if (ch === '/' && text[pos + 1] === '*') {
            accumulateTextBlock();
            scanMultilineComment();
            continue;
        }
        // Non-whitespace, non-comment, must be regular token. End if we're not scanning
        // to a particular location, otherwise continue.
        if (stopAtCode) {
            break;
        }
        pos++;
    }
    accumulateTextBlock();
    return ret;
    function scanMultilineComment() {
        const endOfComment = findNext('*/', pos + 2);
        ret.push({
            type: 'blockcomment',
            hasTrailingNewLine: ['\n', '\r'].includes(text[endOfComment + 2]),
            pos,
            end: endOfComment + 2,
        });
        pos = endOfComment + 2;
        start = pos;
    }
    function scanSinglelineComment() {
        const nl = Math.min(findNext('\r', pos + 2), findNext('\n', pos + 2));
        if (text[pos + 2] === '/') {
            // Special /// comment
            ret.push({
                type: 'directive',
                hasTrailingNewLine: true,
                pos: pos + 1,
                end: nl,
            });
        }
        else {
            // Regular // comment
            ret.push({
                type: 'linecomment',
                hasTrailingNewLine: true,
                pos,
                end: nl,
            });
        }
        pos = nl + 1;
        start = pos;
    }
    function accumulateTextBlock() {
        if (pos - start > 0) {
            ret.push({
                type: 'other',
                hasTrailingNewLine: false,
                pos: start,
                end: pos,
            });
            start = pos;
        }
    }
    function findNext(sub, startPos) {
        const f = text.indexOf(sub, startPos);
        if (f === -1) {
            return text.length;
        }
        return f;
    }
}
const VOID_SHOW_KEYWORD = 'show';
function extractMaskingVoidExpression(node) {
    const expr = extractVoidExpression(node);
    if (!expr) {
        return undefined;
    }
    if (ts.isStringLiteral(expr.expression) && expr.expression.text === VOID_SHOW_KEYWORD) {
        return undefined;
    }
    return expr;
}
function extractShowingVoidExpression(node) {
    const expr = extractVoidExpression(node);
    if (!expr) {
        return undefined;
    }
    if (ts.isStringLiteral(expr.expression) && expr.expression.text === VOID_SHOW_KEYWORD) {
        return expr;
    }
    return undefined;
}
/**
 * Return the string argument to a void expression if it exists
 */
function voidExpressionString(node) {
    if (ts.isStringLiteral(node.expression)) {
        return node.expression.text;
    }
    return undefined;
}
/**
 * We use void directives as pragmas. Extract the void directives here
 */
function extractVoidExpression(node) {
    if (ts.isVoidExpression(node)) {
        return node;
    }
    if (ts.isExpressionStatement(node)) {
        return extractVoidExpression(node.expression);
    }
    if (ts.isParenthesizedExpression(node)) {
        return extractVoidExpression(node.expression);
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.CommaToken) {
        return extractVoidExpression(node.left);
    }
    return undefined;
}
function quoteStringLiteral(x) {
    return x.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}
function visibility(x) {
    const flags = ts.getCombinedModifierFlags(x);
    if (flags & ts.ModifierFlags.Private) {
        return 'private';
    }
    if (flags & ts.ModifierFlags.Protected) {
        return 'protected';
    }
    return 'public';
}
function hasFlag(flag) {
    return (x) => {
        const flags = ts.getCombinedModifierFlags(x);
        return (flags & flag) !== 0;
    };
}
exports.isReadOnly = hasFlag(ts.ModifierFlags.Readonly);
exports.isExported = hasFlag(ts.ModifierFlags.Export);
exports.isPrivate = hasFlag(ts.ModifierFlags.Private);
exports.isProtected = hasFlag(ts.ModifierFlags.Private);
function isPublic(x) {
    // In TypeScript, anything not explicitly marked private or protected is public.
    return !(0, exports.isPrivate)(x) && !(0, exports.isProtected)(x);
}
exports.isStatic = hasFlag(ts.ModifierFlags.Static);
/**
 * Return the super() call from a method body if found
 */
function findSuperCall(node, renderer) {
    if (node === undefined) {
        return undefined;
    }
    if (ts.isCallExpression(node)) {
        if (renderer.textOf(node.expression) === 'super') {
            return node;
        }
    }
    if (ts.isExpressionStatement(node)) {
        return findSuperCall(node.expression, renderer);
    }
    if (ts.isBlock(node)) {
        for (const statement of node.statements) {
            if (ts.isExpressionStatement(statement)) {
                const s = findSuperCall(statement.expression, renderer);
                if (s) {
                    return s;
                }
            }
        }
    }
    return undefined;
}
/**
 * Return the names of all private property declarations
 */
function privatePropertyNames(members, renderer) {
    const props = members.filter((m) => ts.isPropertyDeclaration(m));
    return props.filter((m) => visibility(m) === 'private').map((m) => renderer.textOf(m.name));
}
function findEnclosingClassDeclaration(node) {
    while (node && !ts.isClassDeclaration(node)) {
        node = node.parent;
    }
    return node;
}
//# sourceMappingURL=ast-utils.js.map