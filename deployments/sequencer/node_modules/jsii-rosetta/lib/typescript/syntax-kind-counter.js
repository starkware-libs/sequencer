"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.SyntaxKindCounter = void 0;
const ts = require("typescript");
class SyntaxKindCounter {
    constructor(visibleSpans) {
        this.visibleSpans = visibleSpans;
        this.counter = {};
    }
    countKinds(sourceFile) {
        this.countNode(sourceFile);
        return this.counter;
    }
    countNode(node) {
        if (this.visibleSpans.containsStartOfNode(node)) {
            this.counter[node.kind] = (this.counter[node.kind] ?? 0) + 1;
        }
        // The two recursive options produce differing results. `ts.forEachChild()` ignores some unimportant kinds.
        // `node.getChildren()` goes through all syntax kinds.
        // see: https://basarat.gitbook.io/typescript/overview/ast/ast-tip-children
        ts.forEachChild(node, (x) => this.countNode(x));
    }
}
exports.SyntaxKindCounter = SyntaxKindCounter;
//# sourceMappingURL=syntax-kind-counter.js.map