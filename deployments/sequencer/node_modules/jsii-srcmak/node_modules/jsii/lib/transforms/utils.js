"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.combinedTransformers = combinedTransformers;
/**
 * Combines a collection of `CustomTransformers` configurations into a single
 * one, preserving the order of arguments.
 *
 * @param transformers the list of transformers to combine.
 *
 * @returns the combined transformer.
 */
function combinedTransformers(...transformers) {
    // Note the ! below are just to stop the type checker from seeing undefined as
    // a value for the whole map-filter-reduce chain, as this would require heavy
    // syntax that is not desirable. The filter step removes the `undefined`.
    return {
        before: transformers
            .map((transformer) => transformer.before)
            .filter((transform) => transform != null)
            .reduce((acc, elt) => (acc ? [...acc, ...elt] : elt), undefined),
        after: transformers
            .map((transformer) => transformer.after)
            .filter((transform) => transform != null)
            .reduce((acc, elt) => (acc ? [...acc, ...elt] : elt), undefined),
        afterDeclarations: transformers
            .map((transformer) => transformer.afterDeclarations)
            .filter((transform) => transform != null)
            .reduce((acc, elt) => (acc ? [...acc, ...elt] : elt), undefined),
    };
}
//# sourceMappingURL=utils.js.map