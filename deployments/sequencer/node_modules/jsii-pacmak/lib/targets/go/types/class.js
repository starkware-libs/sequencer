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
var _GoClass_parameterValidators, _GoClassConstructor_validator;
Object.defineProperty(exports, "__esModule", { value: true });
exports.StaticMethod = exports.ClassMethod = exports.GoClassConstructor = exports.GoClass = void 0;
const naming_util_1 = require("../../../naming-util");
const comparators = require("../comparators");
const runtime_1 = require("../runtime");
const runtime_type_checking_1 = require("../runtime/runtime-type-checking");
const util_1 = require("../util");
const go_type_1 = require("./go-type");
const go_type_reference_1 = require("./go-type-reference");
const type_member_1 = require("./type-member");
/*
 * GoClass wraps a Typescript class as a Go custom struct type
 */
class GoClass extends go_type_1.GoType {
    constructor(pkg, type) {
        super(pkg, type);
        _GoClass_parameterValidators.set(this, void 0);
        const methods = new Array();
        const staticMethods = new Array();
        for (const method of type.allMethods) {
            if (method.static) {
                staticMethods.push(new StaticMethod(this, method));
            }
            else {
                methods.push(new ClassMethod(this, method));
            }
        }
        // Ensure consistent order, mostly cosmetic.
        this.methods = methods.sort(comparators.byName);
        this.staticMethods = staticMethods.sort(comparators.byName);
        const properties = new Array();
        const staticProperties = new Array();
        for (const prop of type.allProperties) {
            if (prop.static) {
                staticProperties.push(new type_member_1.GoProperty(this, prop));
            }
            else {
                properties.push(new type_member_1.GoProperty(this, prop));
            }
        }
        // Ensure consistent order, mostly cosmetic.
        this.properties = properties.sort(comparators.byName);
        this.staticProperties = staticProperties.sort(comparators.byName);
        if (type.initializer) {
            this.initializer = new GoClassConstructor(this, type.initializer);
        }
    }
    get parameterValidators() {
        if (__classPrivateFieldGet(this, _GoClass_parameterValidators, "f") === undefined) {
            __classPrivateFieldSet(this, _GoClass_parameterValidators, [
                ...this.methods.map((m) => m.validator).filter((v) => v != null),
                ...this.staticMethods.map((m) => m.validator).filter((v) => v != null),
                ...this.properties.map((m) => m.validator).filter((v) => v != null),
                ...this.staticProperties
                    .map((m) => m.validator)
                    .filter((v) => v != null),
                ...(this.initializer?.validator ? [this.initializer.validator] : []),
            ], "f");
        }
        return __classPrivateFieldGet(this, _GoClass_parameterValidators, "f");
    }
    get extends() {
        // Cannot compute in constructor, as dependencies may not have finished
        // resolving just yet.
        if (this._extends === undefined) {
            this._extends = this.type.base
                ? this.pkg.root.findType(this.type.base.fqn)
                : null;
        }
        return this._extends ?? undefined;
    }
    get implements() {
        // Cannot compute in constructor, as dependencies may not have finished
        // resolving just yet.
        this._implements ?? (this._implements = this.type.interfaces
            .map((iface) => this.pkg.root.findType(iface.fqn))
            // Ensure consistent order, mostly cosmetic.
            .sort((l, r) => l.fqn.localeCompare(r.fqn)));
        return this._implements;
    }
    get baseTypes() {
        return [...(this.extends ? [this.extends] : []), ...this.implements];
    }
    emit(context) {
        this.emitInterface(context);
        this.emitStruct(context);
        this.emitGetters(context);
        if (this.initializer) {
            this.initializer.emit(context);
        }
        this.emitSetters(context);
        for (const method of this.staticMethods) {
            method.emit(context);
        }
        for (const prop of this.staticProperties) {
            prop.emitGetterProxy(context);
            prop.emitSetterProxy(context);
        }
        for (const method of this.methods) {
            method.emit(context);
        }
    }
    emitRegistration({ code }) {
        code.open(`${runtime_1.JSII_RT_ALIAS}.RegisterClass(`);
        code.line(`"${this.fqn}",`);
        code.line(`reflect.TypeOf((*${this.name})(nil)).Elem(),`);
        const allMembers = [
            ...this.type.allMethods
                .filter((method) => !method.static)
                .map((method) => new ClassMethod(this, method)),
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
        this.emitProxyMakerFunction(code, this.baseTypes);
        code.close(')');
    }
    get members() {
        return [
            ...(this.initializer ? [this.initializer] : []),
            ...this.methods,
            ...this.properties,
            ...this.staticMethods,
            ...this.staticProperties,
        ];
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: this.initializer != null ||
                this.members.some((m) => m.specialDependencies.init),
            internal: this.baseTypes.some((base) => this.pkg.isExternalType(base)),
            runtime: this.initializer != null || this.members.length > 0,
            time: !!this.initializer?.specialDependencies.time ||
                this.members.some((m) => m.specialDependencies.time),
        };
    }
    emitInterface(context) {
        const { code, documenter } = context;
        documenter.emit(this.type.docs, this.apiLocation);
        code.openBlock(`type ${this.name} interface`);
        // embed extended interfaces
        if (this.extends) {
            code.line(new go_type_reference_1.GoTypeRef(this.pkg.root, this.extends.type.reference).scopedName(this.pkg));
        }
        for (const iface of this.implements) {
            code.line(new go_type_reference_1.GoTypeRef(this.pkg.root, iface.type.reference).scopedName(this.pkg));
        }
        for (const property of this.properties) {
            property.emitGetterDecl(context);
            property.emitSetterDecl(context);
        }
        for (const method of this.methods) {
            method.emitDecl(context);
        }
        code.closeBlock();
        code.line();
    }
    emitGetters(context) {
        if (this.properties.length === 0) {
            return;
        }
        for (const property of this.properties) {
            property.emitGetterProxy(context);
        }
        context.code.line();
    }
    emitStruct({ code }) {
        code.line(`// The jsii proxy struct for ${this.name}`);
        code.openBlock(`type ${this.proxyName} struct`);
        // Make sure this is not 0-width
        if (this.baseTypes.length === 0) {
            code.line('_ byte // padding');
        }
        else {
            for (const base of this.baseTypes) {
                code.line(this.pkg.resolveEmbeddedType(base).embed);
            }
        }
        code.closeBlock();
        code.line();
    }
    // emits the implementation of the setters for the struct
    emitSetters(context) {
        for (const property of this.properties) {
            property.emitSetterProxy(context);
        }
    }
    get dependencies() {
        // need to add dependencies of method arguments and constructor arguments
        return [
            ...this.baseTypes.map((ref) => ref.pkg),
            ...(0, util_1.getMemberDependencies)(this.members),
            ...(0, util_1.getParamDependencies)(this.members.filter(isGoMethod)),
        ];
    }
    /*
     * Get fqns of interfaces the class implements
     */
    get interfaces() {
        return this.type.interfaces.map((iFace) => iFace.fqn);
    }
}
exports.GoClass = GoClass;
_GoClass_parameterValidators = new WeakMap();
class GoClassConstructor extends type_member_1.GoMethod {
    constructor(parent, type) {
        super(parent, type);
        this.parent = parent;
        this.type = type;
        _GoClassConstructor_validator.set(this, null);
        this.constructorRuntimeCall = new runtime_1.ClassConstructor(this);
    }
    get validator() {
        if (__classPrivateFieldGet(this, _GoClassConstructor_validator, "f") === null) {
            __classPrivateFieldSet(this, _GoClassConstructor_validator, runtime_type_checking_1.ParameterValidator.forConstructor(this), "f");
        }
        return __classPrivateFieldGet(this, _GoClassConstructor_validator, "f");
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: true,
            internal: false,
            runtime: true,
            time: this.parameters.some((p) => p.reference.specialDependencies.time),
        };
    }
    emit(context) {
        // Abstract classes cannot be directly created
        if (!this.parent.type.abstract) {
            this.emitNew(context);
        }
        // Subclassable classes (the default) get an _Overrides constructor
        if (this.parent.type.spec.docs?.subclassable ?? true) {
            this.emitOverride(context);
        }
    }
    emitNew(context) {
        const { code, documenter } = context;
        const constr = `New${this.parent.name}`;
        const paramString = this.parameters.length === 0
            ? ''
            : this.parameters.map((p) => p.toString()).join(', ');
        documenter.emit(this.type.docs, this.apiLocation);
        code.openBlock(`func ${constr}(${paramString}) ${this.parent.name}`);
        this.constructorRuntimeCall.emit(context);
        code.closeBlock();
        code.line();
    }
    emitOverride({ code, documenter }) {
        const constr = `New${this.parent.name}_Override`;
        const params = this.parameters.map((p) => p.toString());
        const instanceVar = (0, runtime_1.slugify)(this.parent.name[0].toLowerCase(), params);
        params.unshift(`${instanceVar} ${this.parent.name}`);
        documenter.emit(this.type.docs, this.apiLocation);
        code.openBlock(`func ${constr}(${params.join(', ')})`);
        this.constructorRuntimeCall.emitOverride(code, instanceVar);
        code.closeBlock();
        code.line();
    }
}
exports.GoClassConstructor = GoClassConstructor;
_GoClassConstructor_validator = new WeakMap();
class ClassMethod extends type_member_1.GoMethod {
    constructor(parent, method) {
        super(parent, method);
        this.parent = parent;
        this.method = method;
        this.runtimeCall = new runtime_1.MethodCall(this);
    }
    /* emit generates method implementation on the class */
    emit(context) {
        const name = this.name;
        const returnTypeString = this.reference?.void ? '' : ` ${this.returnType}`;
        const { code } = context;
        code.openBlock(`func (${this.instanceArg} *${this.parent.proxyName}) ${name}(${this.paramString()})${returnTypeString}`);
        this.runtimeCall.emit(context);
        code.closeBlock();
        code.line();
    }
    /* emitDecl generates method declaration in the class interface */
    emitDecl({ code, documenter }) {
        const returnTypeString = this.reference?.void ? '' : ` ${this.returnType}`;
        documenter.emit(this.method.docs, this.apiLocation);
        code.line(`${this.name}(${this.paramString()})${returnTypeString}`);
    }
    get instanceArg() {
        return this.parent.name.substring(0, 1).toLowerCase();
    }
    get static() {
        return !!this.method.spec.static;
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: this.method.static,
            internal: false,
            runtime: true,
            time: !!this.parameters.some((p) => p.reference.specialDependencies.time) ||
                !!this.reference?.specialDependencies.time,
        };
    }
}
exports.ClassMethod = ClassMethod;
class StaticMethod extends ClassMethod {
    constructor(parent, method) {
        super(parent, method);
        this.parent = parent;
        this.method = method;
        this.name = `${this.parent.name}_${(0, naming_util_1.jsiiToPascalCase)(method.name)}`;
    }
    emit(context) {
        const returnTypeString = this.reference?.void ? '' : ` ${this.returnType}`;
        const { code, documenter } = context;
        documenter.emit(this.method.docs, this.apiLocation);
        code.openBlock(`func ${this.name}(${this.paramString()})${returnTypeString}`);
        this.runtimeCall.emit(context);
        code.closeBlock();
        code.line();
    }
}
exports.StaticMethod = StaticMethod;
function isGoMethod(m) {
    return m instanceof type_member_1.GoMethod;
}
//# sourceMappingURL=class.js.map