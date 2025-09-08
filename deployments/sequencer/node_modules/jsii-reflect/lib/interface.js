"use strict";
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _InterfaceType_interfaces;
Object.defineProperty(exports, "__esModule", { value: true });
exports.InterfaceType = void 0;
const method_1 = require("./method");
const property_1 = require("./property");
const reference_type_1 = require("./reference-type");
class InterfaceType extends reference_type_1.ReferenceType {
    constructor(system, assembly, spec) {
        super(system, assembly, spec);
        this.system = system;
        this.assembly = assembly;
        this.spec = spec;
        /** Caches the result of `getInterfaces`. */
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _InterfaceType_interfaces.set(this, new Map());
    }
    /**
     * True if this interface only contains properties. Different backends might
     * have idiomatic ways to allow defining concrete instances such interfaces.
     * For example, in Java, the generator will produce a PoJo and a builder
     * which will allow users to create a concrete object with data which
     * adheres to this interface.
     */
    get datatype() {
        return this.isDataType();
    }
    /**
     * Lists all interfaces this interface extends.
     * @param inherited include all interfaces implemented by all super interfaces (default: false)
     */
    getInterfaces(inherited = false) {
        if (!this.spec.interfaces) {
            return [];
        }
        if (__classPrivateFieldGet(this, _InterfaceType_interfaces, "f").has(inherited)) {
            return Array.from(__classPrivateFieldGet(this, _InterfaceType_interfaces, "f").get(inherited));
        }
        const result = new Set();
        for (const iface of this.spec.interfaces) {
            const ifaceType = this.system.findInterface(iface);
            if (!result.has(ifaceType) && inherited) {
                ifaceType.getInterfaces(inherited).forEach((i) => result.add(i));
            }
            result.add(ifaceType);
        }
        __classPrivateFieldGet(this, _InterfaceType_interfaces, "f").set(inherited, Array.from(result));
        // Returning a copy of the array, distinct from the one we memoized, for safety.
        return Array.from(result);
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
    isDataType() {
        return !!this.spec.datatype;
    }
    isInterfaceType() {
        return true;
    }
    _getProperties(inherited, parentType) {
        const result = new Map();
        if (inherited) {
            for (const parent of this.getInterfaces()) {
                for (const [key, value] of parent._getProperties(inherited, parentType)) {
                    result.set(key, value);
                }
            }
        }
        for (const p of this.spec.properties ?? []) {
            result.set(p.name, new property_1.Property(this.system, this.assembly, parentType, this, p));
        }
        return result;
    }
    _getMethods(inherited, parentType) {
        const methods = new Map();
        if (inherited) {
            for (const parent of this.getInterfaces()) {
                for (const [key, value] of parent._getMethods(inherited, parentType)) {
                    methods.set(key, value);
                }
            }
        }
        for (const m of this.spec.methods ?? []) {
            methods.set(m.name, new method_1.Method(this.system, this.assembly, parentType, this, m));
        }
        return methods;
    }
}
exports.InterfaceType = InterfaceType;
_InterfaceType_interfaces = new WeakMap();
//# sourceMappingURL=interface.js.map