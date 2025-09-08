"use strict";
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.ReferenceType = void 0;
const _memoized_1 = require("./_memoized");
const type_1 = require("./type");
class ReferenceType extends type_1.Type {
    constructor(system, assembly, spec) {
        super(system, assembly, spec);
        this.system = system;
        this.assembly = assembly;
    }
    /**
     * All the base interfaces that this interface extends.
     */
    get interfaces() {
        return this.getInterfaces();
    }
    /**
     * List of methods (without inherited methods).
     */
    get ownMethods() {
        return Object.values(this.getMethods(false));
    }
    /**
     * List of own and inherited methods
     */
    get allMethods() {
        return Object.values(this.getMethods(true));
    }
    /**
     * List of properties.
     */
    get ownProperties() {
        return Object.values(this.getProperties());
    }
    /**
     * List of own and inherited methods
     */
    get allProperties() {
        return Object.values(this.getProperties(true));
    }
    get ownMembers() {
        return Object.values(this.getMembers(false));
    }
    get allMembers() {
        return Object.values(this.getMembers(true));
    }
    getMembers(inherited = false) {
        return Object.assign(this.getMethods(inherited), this.getProperties(inherited));
    }
}
exports.ReferenceType = ReferenceType;
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "interfaces", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "ownMethods", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "allMethods", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "ownProperties", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "allProperties", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "ownMembers", null);
__decorate([
    _memoized_1.memoized
], ReferenceType.prototype, "allMembers", null);
//# sourceMappingURL=reference-type.js.map