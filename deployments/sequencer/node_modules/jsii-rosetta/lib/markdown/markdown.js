"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.transformMarkdown = transformMarkdown;
exports.renderCommonMarkTree = renderCommonMarkTree;
exports.visitCommonMarkTree = visitCommonMarkTree;
exports.prefixLines = prefixLines;
exports.cmNodeChildren = cmNodeChildren;
const cm = require("commonmark");
function transformMarkdown(source, renderer, transform) {
    const parser = new cm.Parser();
    const doc = parser.parse(source);
    if (transform) {
        visitCommonMarkTree(doc, transform);
    }
    return renderCommonMarkTree(doc, renderer);
}
function renderCommonMarkTree(node, renderer) {
    const context = {
        recurse(n) {
            return renderCommonMarkTree(n, renderer);
        },
        content() {
            return this.children().join('');
        },
        children() {
            const parts = [];
            for (const child of cmNodeChildren(node)) {
                parts.push(renderCommonMarkTree(child, renderer));
            }
            return parts;
        },
    };
    return renderer[node.type](node, context);
}
function visitCommonMarkTree(node, visitor) {
    visitor[node.type](node);
    for (const child of cmNodeChildren(node)) {
        visitCommonMarkTree(child, visitor);
    }
}
function prefixLines(prefix, x) {
    return x
        .split('\n')
        .map((l) => prefix + l)
        .join('\n');
}
function* cmNodeChildren(node) {
    for (let child = node.firstChild; child !== null; child = child.next) {
        yield child;
    }
}
//# sourceMappingURL=markdown.js.map