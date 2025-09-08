"use strict";
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _a, _Directives_CACHE;
Object.defineProperty(exports, "__esModule", { value: true });
exports.Directives = void 0;
const ts = require("typescript");
const jsii_diagnostic_1 = require("./jsii-diagnostic");
/**
 * TSDoc-style directives that can be attached to a symbol.
 */
class Directives {
    /**
     * Obtains the `Directives` for a given TypeScript AST node.
     *
     * @param node         the node for which directives are requested.
     * @param onDiagnostic a callback invoked whenever a diagnostic message is
     *                     emitted when parsing directives.
     */
    static of(node, onDiagnostic) {
        const found = __classPrivateFieldGet(_a, _a, "f", _Directives_CACHE).get(node);
        if (found != null) {
            return found;
        }
        const directives = new _a(node, onDiagnostic);
        __classPrivateFieldGet(_a, _a, "f", _Directives_CACHE).set(node, directives);
        return directives;
    }
    constructor(node, onDiagnostic) {
        for (const tag of ts.getJSDocTags(node)) {
            switch (tag.tagName.text) {
                case 'internal':
                    this.tsInternal ?? (this.tsInternal = tag);
                    break;
                case 'jsii':
                    const comments = getComments(tag);
                    if (comments.length === 0) {
                        onDiagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_2000_MISSING_DIRECTIVE_ARGUMENT.create(tag));
                        continue;
                    }
                    for (const { text, jsdocNode } of comments) {
                        switch (text) {
                            case 'ignore':
                                this.ignore ?? (this.ignore = jsdocNode);
                                break;
                            default:
                                onDiagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_2999_UNKNOWN_DIRECTIVE.create(jsdocNode, text));
                                break;
                        }
                    }
                    break;
                default: // Ignore
            }
        }
    }
}
exports.Directives = Directives;
_a = Directives;
_Directives_CACHE = { value: new WeakMap() };
function getComments(tag) {
    if (tag.comment == null) {
        return [];
    }
    if (typeof tag.comment === 'string') {
        const text = tag.comment.trim();
        return text
            ? text.split(/[\n,]/).flatMap((line) => {
                line = line.trim();
                return line ? [{ text: line, jsdocNode: tag }] : [];
            })
            : [];
    }
    // Possible per the type signature in the compiler, however not sure in which conditions.
    return tag.comment.flatMap((jsdocNode) => {
        let text;
        switch (jsdocNode.kind) {
            case ts.SyntaxKind.JSDocText:
                text = jsdocNode.text;
                break;
            case ts.SyntaxKind.JSDocLink:
            case ts.SyntaxKind.JSDocLinkCode:
            case ts.SyntaxKind.JSDocLinkPlain:
                text = jsdocNode.name
                    ? `${jsdocNode.name.getText(jsdocNode.name.getSourceFile())}: ${jsdocNode.text}`
                    : jsdocNode.text;
                break;
        }
        text = text.trim();
        return text ? [{ text, jsdocNode }] : [];
    });
}
//# sourceMappingURL=directives.js.map