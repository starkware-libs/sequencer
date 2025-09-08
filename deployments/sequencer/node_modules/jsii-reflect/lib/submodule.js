"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Submodule = void 0;
const module_like_1 = require("./module-like");
class Submodule extends module_like_1.ModuleLike {
    constructor(system, spec, fqn, submoduleMap, typeMap) {
        super(system);
        this.spec = spec;
        this.fqn = fqn;
        this.submoduleMap = submoduleMap;
        this.typeMap = typeMap;
        this.name = fqn.split('.').pop();
    }
    /**
     * A map of target name to configuration, which is used when generating packages for
     * various languages.
     */
    get targets() {
        return this.spec.targets;
    }
    /**
     * The top-level readme document for this assembly (if any).
     */
    get readme() {
        return this.spec.readme;
    }
}
exports.Submodule = Submodule;
//# sourceMappingURL=submodule.js.map