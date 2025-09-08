"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Docs = void 0;
const spec_1 = require("@jsii/spec");
class Docs {
    constructor(system, target, spec, parentDocs) {
        this.system = system;
        this.target = target;
        this.parentDocs = parentDocs;
        this.docs = spec ?? {};
    }
    /**
     * Returns docstring of summary and remarks
     */
    toString() {
        return [this.docs.summary, this.docs.remarks]
            .filter((txt) => !!txt)
            .join('\n\n');
    }
    get subclassable() {
        return !!this.docs.subclassable;
    }
    /**
     * Return the reason for deprecation of this type
     */
    get deprecationReason() {
        if (this.docs.deprecated !== undefined) {
            return this.docs.deprecated;
        }
        if (this.parentDocs) {
            return this.parentDocs.deprecationReason;
        }
        return undefined;
    }
    /**
     * Return whether this type is deprecated
     */
    get deprecated() {
        return this.deprecationReason !== undefined;
    }
    /**
     * Return the stability of this type
     */
    get stability() {
        return lowestStability(this.docs.stability, this.parentDocs?.stability);
    }
    /**
     * Return any custom tags on this type
     */
    customTag(tag) {
        return this.docs.custom?.[tag];
    }
    /**
     * Return summary of this type
     */
    get summary() {
        return this.docs.summary ?? '';
    }
    /**
     * Return remarks for this type
     */
    get remarks() {
        return this.docs.remarks ?? '';
    }
    /**
     * Return examples for this type
     */
    get example() {
        return this.docs.example ?? '';
    }
    /**
     * Return documentation links for this type
     */
    get link() {
        return this.docs.see ?? '';
    }
    /**
     * Returns the return type
     */
    get returns() {
        return this.docs.returns ?? '';
    }
    /**
     * Returns the default value
     */
    get default() {
        return this.docs.default ?? '';
    }
}
exports.Docs = Docs;
const stabilityPrecedence = {
    [spec_1.Stability.Deprecated]: 0,
    [spec_1.Stability.Experimental]: 1,
    [spec_1.Stability.External]: 2,
    [spec_1.Stability.Stable]: 3,
};
function lowestStability(a, b) {
    if (a === undefined) {
        return b;
    }
    if (b === undefined) {
        return a;
    }
    return stabilityPrecedence[a] < stabilityPrecedence[b] ? a : b;
}
//# sourceMappingURL=docs.js.map