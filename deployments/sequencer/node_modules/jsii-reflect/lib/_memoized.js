"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.memoized = memoized;
exports.memoizedWhenLocked = memoizedWhenLocked;
const CACHE = new WeakMap();
function memoizedGet(original, propertyKey) {
    return function () {
        let cache = CACHE.get(this);
        if (cache == null) {
            cache = new Map();
            CACHE.set(this, cache);
        }
        if (cache.has(propertyKey)) {
            const result = cache.get(propertyKey);
            if (Array.isArray(result)) {
                // Return a copy of arrays as a precaution
                return Array.from(result);
            }
            return result;
        }
        const result = original.call(this);
        // If the result is an array, memoize a copy for safety.
        cache.set(propertyKey, Array.isArray(result) ? Array.from(result) : result);
        return result;
    };
}
/**
 * Decorates property readers for readonly properties so that their results are
 * memoized in a `WeakMap`-based cache. Those properties will consequently be
 * computed exactly once.
 *
 * This can only be applied to property accessors (`public get foo(): any`), and not to
 * property declarations (`public readonly foo: any`).
 *
 * This should not be applied to any computations relying on a typesystem.
 * The typesystem can be changed and thus change the result of the call.
 * Use `memoizedWhenLocked` instead.
 */
function memoized(_prototype, propertyKey, descriptor) {
    if (!descriptor.get) {
        throw new Error(`@memoized can only be applied to property getters!`);
    }
    if (descriptor.set) {
        throw new Error(`@memoized can only be applied to readonly properties!`);
    }
    const original = descriptor.get;
    descriptor.get = memoizedGet(original, propertyKey);
}
function memoizedWhenLocked(_prototype, propertyKey, descriptor) {
    if (!descriptor.get) {
        throw new Error(`@memoized can only be applied to property getters!`);
    }
    if (descriptor.set) {
        throw new Error(`@memoized can only be applied to readonly properties!`);
    }
    const original = descriptor.get;
    descriptor.get = function () {
        if (this.system.isLocked) {
            return memoizedGet(original, propertyKey).call(this);
        }
        return original.call(this);
    };
}
//# sourceMappingURL=_memoized.js.map