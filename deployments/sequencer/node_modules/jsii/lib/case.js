"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.kebab = exports.snake = exports.pascal = exports.constant = exports.camel = void 0;
const Case = require("case");
const withCache = (func) => (text) => Cache.fetch(text, func);
exports.camel = withCache(Case.camel);
exports.constant = withCache(Case.constant);
exports.pascal = withCache(Case.pascal);
exports.snake = withCache(Case.snake);
exports.kebab = withCache(Case.kebab);
class Cache {
    static fetch(text, func) {
        // Check whether we have a cache for this function...
        const cacheKey = CacheKey.for(func);
        let cache = this.CACHES.get(cacheKey);
        if (cache == null) {
            // If not, create one...
            cache = new Map();
            this.CACHES.set(cacheKey, cache);
        }
        // Check if the current cache has a value for this text...
        const cached = cache.get(text);
        if (cached != null) {
            return cached;
        }
        // If not, compute one...
        const result = func(text);
        cache.set(text, result);
        return result;
    }
    constructor() { }
}
// Cache is indexed on a weak CacheKey so the cache can be purged under memory pressure
Cache.CACHES = new WeakMap();
class CacheKey {
    static for(data) {
        const entry = this.STORE.get(data)?.deref();
        if (entry != null) {
            return entry;
        }
        const newKey = new CacheKey();
        this.STORE.set(data, new WeakRef(newKey));
        return newKey;
    }
    constructor() { }
}
// Storing cache keys as weak references to allow garbage collection if there is memory pressure.
CacheKey.STORE = new Map();
//# sourceMappingURL=case.js.map