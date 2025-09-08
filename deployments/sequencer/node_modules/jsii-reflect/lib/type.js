"use strict";
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.Type = void 0;
const _memoized_1 = require("./_memoized");
const docs_1 = require("./docs");
const source_1 = require("./source");
const type_ref_1 = require("./type-ref");
class Type {
    constructor(system, assembly, spec) {
        this.system = system;
        this.assembly = assembly;
        this.spec = spec;
    }
    toString() {
        return `${this.kind} ${this.fqn}`;
    }
    /**
     * The fully qualified name of the type (``<assembly>.<namespace>.<name>``)
     */
    get fqn() {
        return this.spec.fqn;
    }
    /**
     * The namespace of the type (``foo.bar.baz``). When undefined, the type is located at the root of the assembly
     * (it's ``fqn`` would be like ``<assembly>.<name>``). If the `namespace` corresponds to an existing type's
     * namespace-qualified (e.g: ``<namespace>.<name>``), then the current type is a nested type.
     */
    get namespace() {
        return this.spec.namespace;
    }
    /**
     * The type within which this type is nested (if any).
     */
    get nestingParent() {
        const ns = this.namespace;
        if (ns == null) {
            return undefined;
        }
        return this.assembly.tryFindType(`${this.assembly.name}.${ns}`);
    }
    /**
     * The simple name of the type (MyClass).
     */
    get name() {
        return this.spec.name;
    }
    /**
     * The kind of the type.
     */
    get kind() {
        return this.spec.kind;
    }
    get docs() {
        return new docs_1.Docs(this.system, this, this.spec.docs ?? {});
    }
    /**
     * A type reference to this type
     */
    get reference() {
        return new type_ref_1.TypeReference(this.system, {
            fqn: this.fqn,
        });
    }
    /**
     * Determines whether this is a Class type or not.
     */
    isClassType() {
        return false;
    }
    /**
     * Determines whether this is a Data Type (that is, an interface with no methods) or not.
     */
    isDataType() {
        return false; // TODO how is this different from isInterfaceType?
    }
    /**
     * Determines whether this is an Enum type or not.
     */
    isEnumType() {
        return false;
    }
    /**
     * Determines whether this is an Interface type or not.
     */
    isInterfaceType() {
        return false;
    }
    /**
     * Determines whether this type extends a given base or not.
     *
     * @param base the candidate base type.
     */
    extends(base) {
        if (this === base) {
            return true;
        }
        if ((this.isInterfaceType() || this.isClassType()) &&
            base.isInterfaceType()) {
            return this.getInterfaces(true).some((iface) => iface === base);
        }
        if (this.isClassType() && base.isClassType()) {
            return this.ancestors.some((clazz) => clazz === base);
        }
        return false;
    }
    /**
     * Finds all type that:
     * - extend this, if this is a ClassType
     * - implement this, if this is an InterfaceType (this includes interfaces extending this)
     *
     * As classes and interfaces are considered to extend themselves, "this" will be part of all return values when called
     * on classes and interfaces.
     *
     * The result will always be empty for types that are neither ClassType nor InterfaceType.
     */
    get allImplementations() {
        if (this.isClassType() || this.isInterfaceType()) {
            return [
                ...this.system.classes.filter((c) => c.extends(this)),
                ...this.system.interfaces.filter((i) => i.extends(this)),
            ];
        }
        return [];
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
exports.Type = Type;
__decorate([
    _memoized_1.memoized
], Type.prototype, "nestingParent", null);
__decorate([
    _memoized_1.memoized
], Type.prototype, "allImplementations", null);
//# sourceMappingURL=type.js.map