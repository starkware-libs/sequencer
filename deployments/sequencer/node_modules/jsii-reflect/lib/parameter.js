"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Parameter = void 0;
const docs_1 = require("./docs");
const optional_value_1 = require("./optional-value");
class Parameter extends optional_value_1.OptionalValue {
    constructor(system, parentType, method, spec) {
        super(system, spec);
        this.parentType = parentType;
        this.method = method;
        this.spec = spec;
    }
    /**
     * The name of the parameter.
     */
    get name() {
        return this.spec.name;
    }
    /**
     * Whether this argument is the "rest" of a variadic signature.
     * The ``#type`` is that of every individual argument of the variadic list.
     */
    get variadic() {
        return !!this.spec.variadic;
    }
    get docs() {
        return new docs_1.Docs(this.system, this, this.spec.docs ?? {});
    }
}
exports.Parameter = Parameter;
//# sourceMappingURL=parameter.js.map