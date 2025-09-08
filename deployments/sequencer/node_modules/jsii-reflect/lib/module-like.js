"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ModuleLike = void 0;
const class_1 = require("./class");
const enum_1 = require("./enum");
const interface_1 = require("./interface");
class ModuleLike {
    constructor(system) {
        this.system = system;
        /**
         * Cache for the results of `tryFindType`.
         */
        this.typeLocatorCache = new Map();
    }
    get submodules() {
        return Array.from(this.submoduleMap.values());
    }
    /**
     * All types in this module/namespace (not submodules)
     */
    get types() {
        return Array.from(this.typeMap.values());
    }
    /**
     * All classes in this module/namespace (not submodules)
     */
    get classes() {
        return this.types.filter((t) => t instanceof class_1.ClassType).map((t) => t);
    }
    /**
     * All interfaces in this module/namespace (not submodules)
     */
    get interfaces() {
        return this.types.filter((t) => t instanceof interface_1.InterfaceType).map((t) => t);
    }
    /**
     * All enums in this module/namespace (not submodules)
     */
    get enums() {
        return this.types.filter((t) => t instanceof enum_1.EnumType).map((t) => t);
    }
    tryFindType(fqn) {
        if (this.typeLocatorCache.has(fqn)) {
            return this.typeLocatorCache.get(fqn);
        }
        const ownType = this.typeMap.get(fqn);
        if (ownType != null) {
            this.typeLocatorCache.set(fqn, ownType);
            return ownType;
        }
        if (!fqn.startsWith(`${this.fqn}.`)) {
            this.typeLocatorCache.set(fqn, undefined);
            return undefined;
        }
        const myFqnLength = this.fqn.split('.').length;
        const subFqn = fqn
            .split('.')
            .slice(0, myFqnLength + 1)
            .join('.');
        const sub = this.submoduleMap.get(subFqn);
        const submoduleType = sub?.tryFindType(fqn);
        this.typeLocatorCache.set(fqn, submoduleType);
        return submoduleType;
    }
}
exports.ModuleLike = ModuleLike;
//# sourceMappingURL=module-like.js.map