"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.topologicalSort = topologicalSort;
/**
 * Return a topological sort of all elements of xs, according to the given dependency functions
 *
 * Returns tranches of packages that do not have a dependency on each other.
 *
 * Dependencies outside the referenced set are ignored.
 *
 * Not a stable sort, but in order to keep the order as stable as possible, we'll sort by key
 * among elements of equal precedence.
 *
 * @param xs - The elements to sort
 * @param keyFn - Return an element's identifier
 * @param depFn - Return the identifiers of an element's dependencies
 */
function topologicalSort(xs, keyFn, depFn) {
    const remaining = new Map();
    for (const element of xs) {
        const key = keyFn(element);
        remaining.set(key, { key, element, dependencies: depFn(element) });
    }
    const ret = new Array();
    while (remaining.size > 0) {
        // All elements with no more deps in the set can be ordered
        const selectable = Array.from(remaining.values()).filter((e) => e.dependencies.every((d) => !remaining.has(d)));
        selectable.sort((a, b) => (a.key < b.key ? -1 : b.key < a.key ? 1 : 0));
        ret.push(selectable.map((s) => s.element));
        for (const selected of selectable) {
            remaining.delete(selected.key);
        }
        // If we didn't make any progress, we got stuck
        if (selectable.length === 0) {
            throw new Error(`Could not determine ordering between: ${Array.from(remaining.keys()).join(', ')}`);
        }
    }
    return ret;
}
//# sourceMappingURL=toposort.js.map