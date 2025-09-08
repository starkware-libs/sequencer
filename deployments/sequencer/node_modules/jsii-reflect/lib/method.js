"use strict";
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.Method = exports.INITIALIZER_NAME = void 0;
const _memoized_1 = require("./_memoized");
const callable_1 = require("./callable");
const optional_value_1 = require("./optional-value");
const type_member_1 = require("./type-member");
/**
 * Symbolic name for the constructor
 */
exports.INITIALIZER_NAME = '<initializer>';
class Method extends callable_1.Callable {
    static isMethod(x) {
        return x instanceof Method;
    }
    constructor(system, assembly, parentType, definingType, spec) {
        super(system, assembly, parentType, spec);
        this.definingType = definingType;
        this.spec = spec;
        this.kind = type_member_1.MemberKind.Method;
    }
    /**
     * The name of the method.
     */
    get name() {
        return this.spec.name;
    }
    get overrides() {
        if (!this.spec.overrides) {
            return undefined;
        }
        return this.system.findFqn(this.spec.overrides);
    }
    /**
     * The return type of the method (undefined if void or initializer)
     */
    get returns() {
        return new optional_value_1.OptionalValue(this.system, this.spec.returns);
    }
    /**
     * Is this method an abstract method (this means the class will also be an abstract class)
     */
    get abstract() {
        return !!this.spec.abstract;
    }
    /**
     * Is this method asyncrhonous (this means the return value is a promise)
     */
    get async() {
        return !!this.spec.async;
    }
    /**
     * Indicates if this is a static method.
     */
    get static() {
        return !!this.spec.static;
    }
}
exports.Method = Method;
__decorate([
    _memoized_1.memoizedWhenLocked
], Method.prototype, "overrides", null);
//# sourceMappingURL=method.js.map