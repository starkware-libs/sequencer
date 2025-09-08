"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Callable = void 0;
const docs_1 = require("./docs");
const parameter_1 = require("./parameter");
const source_1 = require("./source");
class Callable {
    constructor(system, assembly, parentType, spec) {
        this.system = system;
        this.assembly = assembly;
        this.parentType = parentType;
        this.spec = spec;
    }
    /**
     * The parameters of the method/initializer
     */
    get parameters() {
        return (this.spec.parameters ?? []).map((p) => new parameter_1.Parameter(this.system, this.parentType, this, p));
    }
    /**
     * Indicates if this method is protected (otherwise it is public)
     */
    get protected() {
        return !!this.spec.protected;
    }
    /**
     * Indicates whether this method is variadic or not. When ``true``, the last
     * element of ``#parameters`` will also be flagged ``#variadic``.
     */
    get variadic() {
        return !!this.spec.variadic;
    }
    get overrides() {
        if (!this.spec.overrides) {
            return undefined;
        }
        return this.system.findFqn(this.spec.overrides);
    }
    get docs() {
        return new docs_1.Docs(this.system, this, this.spec.docs ?? {}, this.parentType.docs);
    }
    /**
     * Return the location in the module
     */
    get locationInModule() {
        return this.spec.locationInModule;
    }
    /**
     * Return the location in the repository
     */
    get locationInRepository() {
        return (0, source_1.locationInRepository)(this);
    }
    toString() {
        return `${this.kind}:${this.parentType.fqn}.${this.name}`;
    }
}
exports.Callable = Callable;
//# sourceMappingURL=callable.js.map