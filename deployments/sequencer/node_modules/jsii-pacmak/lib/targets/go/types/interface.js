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
var _GoInterface_parameterValidators;
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoInterface = void 0;
const comparators = require("../comparators");
const dependencies_1 = require("../dependencies");
const runtime_1 = require("../runtime");
const util_1 = require("../util");
const go_type_1 = require("./go-type");
const go_type_reference_1 = require("./go-type-reference");
const type_member_1 = require("./type-member");
class GoInterface extends go_type_1.GoType {
    constructor(pkg, type) {
        super(pkg, type);
        _GoInterface_parameterValidators.set(this, void 0);
        this.methods = type.ownMethods
            .map((method) => new InterfaceMethod(this, method))
            .sort(comparators.byName);
        this.properties = type.ownProperties
            .map((prop) => new InterfaceProperty(this, prop))
            .sort(comparators.byName);
        // If there is more than one base, and any ancestor (including transitive)
        // comes from a different assembly, we will re-implement all members on the
        // proxy struct, as otherwise we run the risk of un-promotable methods
        // caused by inheriting the same interface via multiple paths (since we have
        // to represent those as embedded types).
        if (type.interfaces.length > 1 &&
            type
                .getInterfaces(true)
                .some((ancestor) => ancestor.assembly.fqn !== type.assembly.fqn)) {
            this.reimplementedMethods = type.allMethods
                .filter((method) => !method.static && method.definingType !== type)
                .map((method) => new InterfaceMethod(this, method))
                .sort(comparators.byName);
            this.reimplementedProperties = type.allProperties
                .filter((property) => !property.static && property.definingType !== type)
                .map((property) => new InterfaceProperty(this, property))
                .sort(comparators.byName);
        }
        else {
            this.reimplementedMethods = [];
            this.reimplementedProperties = [];
        }
    }
    get parameterValidators() {
        if (__classPrivateFieldGet(this, _GoInterface_parameterValidators, "f") == null) {
            __classPrivateFieldSet(this, _GoInterface_parameterValidators, [
                ...this.methods.map((m) => m.validator).filter((v) => v != null),
                ...this.reimplementedMethods
                    .map((m) => m.validator)
                    .filter((v) => v != null),
                ...this.properties.map((p) => p.validator).filter((v) => v != null),
                ...this.reimplementedProperties
                    .map((p) => p.validator)
                    .filter((v) => v != null),
            ], "f");
        }
        return __classPrivateFieldGet(this, _GoInterface_parameterValidators, "f");
    }
    emit(context) {
        this.emitDocs(context);
        const { code } = context;
        code.openBlock(`type ${this.name} interface`);
        // embed extended interfaces
        for (const iface of this.extends) {
            code.line(new go_type_reference_1.GoTypeRef(this.pkg.root, iface.type.reference).scopedName(this.pkg));
        }
        for (const method of this.methods) {
            method.emitDecl(context);
        }
        for (const prop of this.properties) {
            prop.emit(context);
        }
        code.closeBlock();
        code.line();
        code.line(`// The jsii proxy for ${this.name}`);
        code.openBlock(`type ${this.proxyName} struct`);
        if (this.extends.length === 0) {
            // Ensure this is not 0-width
            code.line('_ byte // padding');
        }
        else {
            for (const base of this.extends) {
                code.line(this.pkg.resolveEmbeddedType(base).embed);
            }
        }
        code.closeBlock();
        code.line();
        for (const method of this.methods) {
            method.emit(context);
        }
        for (const method of this.reimplementedMethods) {
            method.emit(context);
        }
        for (const prop of this.properties) {
            prop.emitGetterProxy(context);
            if (!prop.immutable) {
                prop.emitSetterProxy(context);
            }
        }
        for (const prop of this.reimplementedProperties) {
            prop.emitGetterProxy(context);
            if (!prop.immutable) {
                prop.emitSetterProxy(context);
            }
        }
    }
    emitRegistration({ code }) {
        code.open(`${runtime_1.JSII_RT_ALIAS}.RegisterInterface(`);
        code.line(`"${this.fqn}",`);
        code.line(`reflect.TypeOf((*${this.name})(nil)).Elem(),`);
        const allMembers = [
            ...this.type.allMethods
                .filter((method) => !method.static)
                .map((method) => new InterfaceMethod(this, method)),
            ...this.type.allProperties
                .filter((property) => !property.static)
                .map((property) => new type_member_1.GoProperty(this, property)),
        ].sort(comparators.byName);
        if (allMembers.length === 0) {
            code.line('nil, // no members');
        }
        else {
            code.open(`[]${runtime_1.JSII_RT_ALIAS}.Member{`);
            for (const member of allMembers) {
                code.line(`${member.override},`);
            }
            code.close('},');
        }
        this.emitProxyMakerFunction(code, this.extends);
        code.close(')');
    }
    get specialDependencies() {
        return (0, dependencies_1.reduceSpecialDependencies)({
            fmt: false,
            init: false,
            internal: this.extends.some((base) => this.pkg.isExternalType(base)),
            runtime: false,
            time: false,
        }, ...this.properties.map((p) => p.specialDependencies), ...this.reimplementedProperties.map((p) => p.specialDependencies), ...this.methods.map((m) => m.specialDependencies), ...this.reimplementedMethods.map((m) => m.specialDependencies));
    }
    get extends() {
        return this.type.interfaces
            .map((iface) => this.pkg.root.findType(iface.fqn))
            .sort(comparators.byName);
    }
    get extendsDependencies() {
        const packages = [];
        for (const ifaceRef of this.extends) {
            const pkg = ifaceRef.pkg;
            if (pkg) {
                packages.push(pkg);
            }
        }
        return packages;
    }
    get dependencies() {
        return [
            ...this.extendsDependencies,
            ...(0, util_1.getMemberDependencies)(this.methods),
            ...(0, util_1.getMemberDependencies)(this.reimplementedMethods),
            ...(0, util_1.getMemberDependencies)(this.properties),
            ...(0, util_1.getMemberDependencies)(this.reimplementedProperties),
            ...(0, util_1.getParamDependencies)(this.methods),
            ...(0, util_1.getParamDependencies)(this.reimplementedMethods),
        ];
    }
}
exports.GoInterface = GoInterface;
_GoInterface_parameterValidators = new WeakMap();
class InterfaceProperty extends type_member_1.GoProperty {
    constructor(parent, property) {
        super(parent, property);
        this.parent = parent;
        this.property = property;
    }
    get returnType() {
        return this.reference.scopedReference(this.parent.pkg);
    }
    emit({ code, documenter }) {
        documenter.emit(this.property.docs, this.apiLocation);
        code.line(`${this.name}() ${this.returnType}`);
        if (!this.property.immutable) {
            // For setters, only emit the stability. Copying the documentation from
            // the getter might result in confusing documentation. This is an "okay"
            // middle-ground.
            documenter.emitStability(this.property.docs);
            code.line(`Set${this.name}(${this.name[0].toLowerCase()} ${this.returnType})`);
        }
    }
}
class InterfaceMethod extends type_member_1.GoMethod {
    constructor(parent, method) {
        super(parent, method);
        this.parent = parent;
        this.method = method;
        this.runtimeCall = new runtime_1.MethodCall(this);
    }
    emitDecl({ code, documenter }) {
        documenter.emit(this.method.docs, this.apiLocation);
        code.line(`${this.name}(${this.paramString()})${this.returnTypeString}`);
    }
    emit(context) {
        const name = this.name;
        const { code } = context;
        code.openBlock(`func (${this.instanceArg} *${this.parent.proxyName}) ${name}(${this.paramString()})${this.returnTypeString}`);
        this.runtimeCall.emit(context);
        code.closeBlock();
        code.line();
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: false,
            internal: false,
            runtime: true,
            time: this.parameters.some((p) => p.reference.specialDependencies.time) ||
                !!this.reference?.specialDependencies.time,
        };
    }
    get returnTypeString() {
        return this.reference?.void ? '' : ` ${this.returnType}`;
    }
}
//# sourceMappingURL=interface.js.map