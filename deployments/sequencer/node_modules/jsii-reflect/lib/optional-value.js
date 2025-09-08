"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.OptionalValue = void 0;
const type_ref_1 = require("./type-ref");
class OptionalValue {
    static describe(optionalValue) {
        let description = optionalValue.type.toString();
        if (optionalValue.optional && !optionalValue.type.isAny) {
            description = `Optional<${description}>`;
        }
        return description;
    }
    constructor(system, spec) {
        this.system = system;
        this.spec = spec;
    }
    toString() {
        return OptionalValue.describe(this);
    }
    get type() {
        return new type_ref_1.TypeReference(this.system, this.spec?.type);
    }
    get optional() {
        return !!this.spec?.optional;
    }
}
exports.OptionalValue = OptionalValue;
//# sourceMappingURL=optional-value.js.map