"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.stabilityPrefixFor = stabilityPrefixFor;
exports.renderSummary = renderSummary;
const spec = require("@jsii/spec");
function stabilityPrefixFor(element) {
    if (element.docs?.stability === spec.Stability.Experimental) {
        return '(experimental) ';
    }
    if (element.docs?.stability === spec.Stability.Deprecated) {
        return '(deprecated) ';
    }
    return '';
}
function renderSummary(docs) {
    return docs?.summary ? stabilityPrefixFor({ docs }) + docs.summary : '';
}
//# sourceMappingURL=_utils.js.map