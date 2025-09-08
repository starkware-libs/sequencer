"use strict";
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.ClassType = void 0;
const _memoized_1 = require("./_memoized");
const initializer_1 = require("./initializer");
const method_1 = require("./method");
const property_1 = require("./property");
const reference_type_1 = require("./reference-type");
class ClassType extends reference_type_1.ReferenceType {
    constructor(system, assembly, spec) {
        super(system, assembly, spec);
        this.system = system;
        this.assembly = assembly;
        this.spec = spec;
    }
    /**
     * Base class (optional).
     */
    get base() {
        if (!this.spec.base) {
            return undefined;
        }
        const type = this.system.findFqn(this.spec.base);
        if (!(type instanceof ClassType)) {
            throw new Error(`FQN for base class points to a non-class type: ${this.spec.base}`);
        }
        return type;
    }
    /**
     * Initializer (constructor) method.
     */
    get initializer() {
        if (!this.spec.initializer) {
            return undefined;
        }
        return new initializer_1.Initializer(this.system, this.assembly, this, this.spec.initializer);
    }
    /**
     * Indicates if this class is an abstract class.
     */
    get abstract() {
        return !!this.spec.abstract;
    }
    /**
     * Returns list of all base classes (first is the direct base and last is the top-most).
     *
     * @deprecated use ClassType.ancestors instead
     */
    getAncestors() {
        return this.ancestors;
    }
    /**
     * Returns list of all base classes (first is the direct base and last is the top-most).
     */
    get ancestors() {
        const out = new Array();
        if (this.base) {
            out.push(this.base);
            out.push(...this.base.ancestors);
        }
        return out;
    }
    /**
     * Lists all properties in this class.
     * @param inherited include all properties inherited from base classes (default: false)
     */
    getProperties(inherited = false) {
        return Object.fromEntries(this._getProperties(inherited, this));
    }
    /**
     * List all methods in this class.
     * @param inherited include all methods inherited from base classes (default: false)
     */
    getMethods(inherited = false) {
        return Object.fromEntries(this._getMethods(inherited, this));
    }
    /**
     * Lists all interfaces this class implements.
     * @param inherited include all interfaces implemented by all base classes (default: false)
     */
    getInterfaces(inherited = false) {
        const out = new Array();
        if (inherited && this.base) {
            out.push(...this.base.getInterfaces(inherited));
        }
        if (this.spec.interfaces) {
            out.push(...flatten(this.spec.interfaces
                .map((fqn) => this.system.findInterface(fqn))
                .map((iface) => [
                iface,
                ...(inherited ? iface.getInterfaces(true) : []),
            ])));
        }
        return out;
    }
    isClassType() {
        return true;
    }
    _getProperties(inherited, parentType) {
        const result = inherited && this.base
            ? this.base._getProperties(inherited, parentType)
            : new Map();
        for (const p of this.spec.properties ?? []) {
            result.set(p.name, new property_1.Property(this.system, this.assembly, parentType, this, p));
        }
        return result;
    }
    _getMethods(inherited, parentType) {
        const result = inherited && this.base
            ? this.base._getMethods(inherited, parentType)
            : new Map();
        for (const m of this.spec.methods ?? []) {
            result.set(m.name, new method_1.Method(this.system, this.assembly, parentType, this, m));
        }
        return result;
    }
}
exports.ClassType = ClassType;
__decorate([
    _memoized_1.memoizedWhenLocked
], ClassType.prototype, "base", null);
__decorate([
    _memoized_1.memoizedWhenLocked
], ClassType.prototype, "ancestors", null);
function flatten(xs) {
    return Array.prototype.concat([], ...xs);
}
//# sourceMappingURL=class.js.map