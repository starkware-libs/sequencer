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
var _GoProperty_validator, _GoMethod_validator;
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoParameter = exports.GoMethod = exports.GoProperty = void 0;
const jsii_reflect_1 = require("jsii-reflect");
const naming_util_1 = require("../../../naming-util");
const runtime_1 = require("../runtime");
const runtime_type_checking_1 = require("../runtime/runtime-type-checking");
const util_1 = require("../util");
const index_1 = require("./index");
/*
 * GoProperty encapsulates logic for public properties on a concrete struct, which could represent
 either a JSII class proxy or datatype interface proxy
*/
class GoProperty {
    constructor(parent, property) {
        this.parent = parent;
        this.property = property;
        _GoProperty_validator.set(this, null);
        const localName = (0, naming_util_1.jsiiToPascalCase)(this.property.name);
        this.name = property.spec.static
            ? `${parent.name}_${localName}`
            : localName;
        this.setterName = property.spec.static
            ? `${parent.name}_Set${localName}`
            : `Set${this.name}`;
        this.immutable = property.immutable;
        this.apiLocation = {
            api: 'member',
            fqn: this.parent.fqn,
            memberName: this.property.name,
        };
    }
    get validator() {
        if (__classPrivateFieldGet(this, _GoProperty_validator, "f") === null) {
            __classPrivateFieldSet(this, _GoProperty_validator, runtime_type_checking_1.ParameterValidator.forProperty(this), "f");
        }
        return __classPrivateFieldGet(this, _GoProperty_validator, "f");
    }
    get reference() {
        return new index_1.GoTypeRef(this.parent.pkg.root, this.property.type);
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: this.static,
            internal: false,
            runtime: true,
            time: !!this.reference?.specialDependencies.time,
        };
    }
    get static() {
        return !!this.property.static;
    }
    get returnType() {
        return (this.reference?.scopedReference(this.parent.pkg) ??
            this.property.type.toString());
    }
    get instanceArg() {
        return this.parent.proxyName.substring(0, 1).toLowerCase();
    }
    get override() {
        return `${runtime_1.JSII_RT_ALIAS}.MemberProperty{JsiiProperty: "${this.property.name}", GoGetter: "${this.name}"}`;
    }
    emitStructMember({ code, documenter }) {
        documenter.emit(this.property.docs, this.apiLocation);
        const memberType = this.reference?.type?.name === this.parent.name
            ? `*${this.returnType}`
            : this.returnType;
        const requiredOrOptional = this.property.optional ? 'optional' : 'required';
        // Adds json and yaml tags for easy deserialization
        code.line(`${this.name} ${memberType} \`field:"${requiredOrOptional}" json:"${this.property.name}" yaml:"${this.property.name}"\``);
        // TODO add newline if not the last member
    }
    emitGetterDecl({ code, documenter }) {
        documenter.emit(this.property.docs, this.apiLocation);
        code.line(`${this.name}() ${this.returnType}`);
    }
    emitSetterDecl({ code, documenter }) {
        if (!this.immutable) {
            // For setters, only emit the stability. Copying the documentation from
            // the getter might result in confusing documentation. This is an "okay"
            // middle-ground.
            documenter.emitStability(this.property.docs);
            code.line(`${this.setterName}(val ${this.returnType})`);
        }
    }
    // Emits getter methods on the struct for each property
    emitGetterProxy(context) {
        const { code } = context;
        if (!this.static) {
            const receiver = this.parent.proxyName;
            const instanceArg = receiver.substring(0, 1).toLowerCase();
            code.openBlock(`func (${instanceArg} *${receiver}) ${this.name}() ${this.returnType}`);
            new runtime_1.GetProperty(this).emit(code);
        }
        else {
            code.openBlock(`func ${this.name}() ${this.returnType}`);
            new runtime_1.StaticGetProperty(this).emit(code);
        }
        code.closeBlock();
        code.line();
    }
    emitSetterProxy(context) {
        if (!this.immutable) {
            const { code } = context;
            if (!this.static) {
                const receiver = this.parent.proxyName;
                const instanceArg = receiver.substring(0, 1).toLowerCase();
                code.openBlock(`func (${instanceArg} *${receiver})${this.setterName}(val ${this.returnType})`);
                new runtime_1.SetProperty(this).emit(context);
            }
            else {
                code.openBlock(`func ${this.setterName}(val ${this.returnType})`);
                new runtime_1.StaticSetProperty(this).emit(context);
            }
            code.closeBlock();
            code.line();
        }
    }
}
exports.GoProperty = GoProperty;
_GoProperty_validator = new WeakMap();
class GoMethod {
    constructor(parent, method) {
        this.parent = parent;
        this.method = method;
        _GoMethod_validator.set(this, null);
        this.name = (0, naming_util_1.jsiiToPascalCase)(method.name);
        this.parameters = this.method.parameters.map((param) => new GoParameter(parent, param));
        this.apiLocation =
            method.kind === jsii_reflect_1.MemberKind.Initializer
                ? { api: 'initializer', fqn: parent.fqn }
                : { api: 'member', fqn: parent.fqn, memberName: method.name };
    }
    get validator() {
        if (__classPrivateFieldGet(this, _GoMethod_validator, "f") === null) {
            __classPrivateFieldSet(this, _GoMethod_validator, runtime_type_checking_1.ParameterValidator.forMethod(this), "f");
        }
        return __classPrivateFieldGet(this, _GoMethod_validator, "f");
    }
    get reference() {
        if (jsii_reflect_1.Method.isMethod(this.method) && this.method.returns.type) {
            return new index_1.GoTypeRef(this.parent.pkg.root, this.method.returns.type);
        }
        return undefined;
    }
    get returnsRef() {
        if (
        // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
        this.reference?.type?.type.isClassType() ||
            this.reference?.type?.type.isInterfaceType()) {
            return true;
        }
        return false;
    }
    get returnType() {
        return (this.reference?.scopedReference(this.parent.pkg) ?? this.method.toString());
    }
    get instanceArg() {
        return this.parent.name.substring(0, 1).toLowerCase();
    }
    get override() {
        return `${runtime_1.JSII_RT_ALIAS}.MemberMethod{JsiiMethod: "${this.method.name}", GoMethod: "${this.name}"}`;
    }
    get static() {
        return false;
    }
    paramString() {
        return this.parameters.length === 0
            ? ''
            : this.parameters.map((p) => p.toString()).join(', ');
    }
}
exports.GoMethod = GoMethod;
_GoMethod_validator = new WeakMap();
class GoParameter {
    constructor(parent, parameter) {
        this.name = (0, util_1.substituteReservedWords)(parameter.name);
        this.isOptional = parameter.optional;
        this.isVariadic = parameter.variadic;
        this.type = parameter.type;
        this.pkg = parent.pkg;
    }
    get reference() {
        return new index_1.GoTypeRef(this.pkg.root, this.type);
    }
    toString() {
        const paramType = this.reference.scopedReference(this.pkg);
        return `${this.name} ${this.isVariadic ? '...' : ''}${paramType}`;
    }
}
exports.GoParameter = GoParameter;
//# sourceMappingURL=type-member.js.map