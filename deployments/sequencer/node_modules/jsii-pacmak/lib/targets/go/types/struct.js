"use strict";
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};
var _Struct_structValidator, _Struct_validators;
Object.defineProperty(exports, "__esModule", { value: true });
exports.Struct = void 0;
const assert = require("assert");
const runtime_1 = require("../runtime");
const runtime_type_checking_1 = require("../runtime/runtime-type-checking");
const util_1 = require("../util");
const go_type_1 = require("./go-type");
const type_member_1 = require("./type-member");
/*
 * Struct wraps a JSII datatype interface aka, structs
 */
class Struct extends go_type_1.GoType {
    constructor(parent, type) {
        super(parent, type);
        _Struct_structValidator.set(this, void 0);
        _Struct_validators.set(this, void 0);
        assert(type.isDataType(), `The provided interface ${type.fqn} is not a struct!`);
        this.properties = type.allProperties.map((prop) => new type_member_1.GoProperty(this, prop));
    }
    get parameterValidators() {
        if (__classPrivateFieldGet(this, _Struct_validators, "f") == null) {
            __classPrivateFieldSet(this, _Struct_validators, this.properties
                .map((p) => p.validator)
                .filter((v) => v != null), "f");
        }
        return __classPrivateFieldGet(this, _Struct_validators, "f");
    }
    get structValidator() {
        if (__classPrivateFieldGet(this, _Struct_structValidator, "f") === null) {
            __classPrivateFieldSet(this, _Struct_structValidator, runtime_type_checking_1.StructValidator.for(this), "f");
        }
        return __classPrivateFieldGet(this, _Struct_structValidator, "f");
    }
    get dependencies() {
        return (0, util_1.getMemberDependencies)(this.properties);
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: false,
            internal: false,
            runtime: false,
            time: this.properties.some((prop) => prop.specialDependencies.time),
        };
    }
    emit(context) {
        const { code, documenter } = context;
        documenter.emit(this.type.docs, this.apiLocation);
        code.openBlock(`type ${this.name} struct`);
        for (const property of this.properties) {
            property.emitStructMember(context);
        }
        code.closeBlock();
        code.line();
    }
    emitRegistration({ code, runtimeTypeChecking }) {
        code.open(`${runtime_1.JSII_RT_ALIAS}.RegisterStruct(`);
        code.line(`"${this.fqn}",`);
        code.line(`reflect.TypeOf((*${this.name})(nil)).Elem(),`);
        code.close(')');
        if (runtimeTypeChecking && this.structValidator) {
            code.open(`${runtime_1.JSII_RT_ALIAS}.RegisterStructValidator(`);
            code.line(`reflect.TypeOf((*${this.name})(nil)).Elem(),`);
            code.open('func (i interface{}, d func() string) error {');
            code.line(`return (i.(*${this.name})).validate(d)`);
            code.close('},');
            code.close(')');
        }
    }
}
exports.Struct = Struct;
_Struct_structValidator = new WeakMap(), _Struct_validators = new WeakMap();
//# sourceMappingURL=struct.js.map