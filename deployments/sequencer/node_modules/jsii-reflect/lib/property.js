"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Property = void 0;
const docs_1 = require("./docs");
const optional_value_1 = require("./optional-value");
const source_1 = require("./source");
const type_member_1 = require("./type-member");
class Property extends optional_value_1.OptionalValue {
    constructor(system, assembly, parentType, definingType, spec) {
        super(system, spec);
        this.assembly = assembly;
        this.parentType = parentType;
        this.definingType = definingType;
        this.spec = spec;
        this.kind = type_member_1.MemberKind.Property;
    }
    toString() {
        return `property:${this.parentType.fqn}.${this.name}`;
    }
    /**
     * The name of the property.
     */
    get name() {
        return this.spec.name;
    }
    /**
     * Indicates if this property only has a getter (immutable).
     */
    get immutable() {
        return !!this.spec.immutable;
    }
    /**
     * Indicates if this property is protected (otherwise it is public)
     */
    get protected() {
        return !!this.spec.protected;
    }
    /**
     * Indicates if this property is abstract
     */
    get abstract() {
        return !!this.spec.abstract;
    }
    /**
     * Indicates if this is a static property.
     */
    get static() {
        return !!this.spec.static;
    }
    /**
     * A hint that indicates that this static, immutable property is initialized
     * during startup. This allows emitting "const" idioms in different target languages.
     * Implies `static` and `immutable`.
     */
    get const() {
        return !!this.spec.const;
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
}
exports.Property = Property;
//# sourceMappingURL=property.js.map