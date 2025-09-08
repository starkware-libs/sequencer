"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.StructValidator = exports.ParameterValidator = void 0;
const crypto_1 = require("crypto");
const jsii_reflect_1 = require("jsii-reflect");
const dependencies_1 = require("../dependencies");
const types_1 = require("../types");
const constants_1 = require("./constants");
class ParameterValidator {
    static forConstructor(ctor) {
        return ParameterValidator.fromParts(`New${ctor.parent.name}`, ctor.parameters);
    }
    static forMethod(method) {
        return ParameterValidator.fromParts(method.name, method.parameters, method.static
            ? undefined
            : {
                name: method.instanceArg,
                type: `*${method.parent.proxyName}`,
            });
    }
    static forProperty(property) {
        if (property.immutable) {
            return undefined;
        }
        return ParameterValidator.fromParts(property.setterName, [
            syntheticParameter(property.parent, 'val', property.reference.reference.spec, property.property.optional),
        ], property.static
            ? undefined
            : {
                name: property.instanceArg,
                type: `*${property.parent.proxyName}`,
            });
    }
    static fromParts(name, parameters, receiver) {
        if (parameters.length === 0) {
            return undefined;
        }
        const parameterValidations = new Map();
        for (const param of parameters) {
            const expr = param.name;
            const descr = `parameter ${param.name}`;
            const validations = new Array();
            if (!param.isOptional && !param.isVariadic) {
                validations.push(Validation.nullCheck(expr, descr, param.reference));
            }
            const validation = Validation.forTypeMap(expr, descr, param.isVariadic
                ? { type: 'array', value: param.reference }
                : param.reference.typeMap);
            if (validation) {
                validations.push(validation);
            }
            if (validations.length !== 0) {
                parameterValidations.set(param, validations);
            }
        }
        if (parameterValidations.size === 0) {
            return undefined;
        }
        return new ParameterValidator(name, parameterValidations, receiver);
    }
    constructor(baseName, validations, receiver) {
        this.receiver = receiver;
        this.name = `validate${baseName}Parameters`;
        this.validations = validations;
        this.parameters = Array.from(validations.keys());
    }
    get dependencies() {
        return [
            ...this.parameters.flatMap((p) => p.reference.withTransparentUnions.dependencies),
            ...Array.from(this.validations.values()).flatMap((vs) => vs.flatMap((v) => v.dependencies)),
        ];
    }
    get specialDependencies() {
        return (0, dependencies_1.reduceSpecialDependencies)(...this.parameters.map((p) => p.reference.specialDependencies), ...Array.from(this.validations.values()).flatMap((vs) => vs.flatMap((v) => v.specialDependencies)));
    }
    emitCall(code) {
        const recv = this.receiver?.name ? `${this.receiver.name}.` : '';
        const params = this.parameters
            .map((p) => (p.isVariadic ? `&${p.name}` : p.name))
            .join(', ');
        code.openBlock(`if err := ${recv}${this.name}(${params}); err != nil`);
        code.line(`panic(err)`);
        code.closeBlock();
    }
    emitImplementation(code, scope, noOp = false) {
        code.openBlock(`func ${this.receiver ? `(${this.receiver.name} ${this.receiver.type}) ` : ''}${this.name}(${this.parameters
            .map((p) => p.isVariadic
            ? `${p.name} *[]${p.reference.scopedReference(scope)}`
            : p.toString())
            .join(', ')}) error`);
        if (noOp) {
            code.line('return nil');
        }
        else {
            for (const [_parameter, validations] of this.validations) {
                for (const validation of validations) {
                    validation.emit(code, scope);
                }
                code.line();
            }
            code.line('return nil');
        }
        code.closeBlock();
        code.line();
    }
}
exports.ParameterValidator = ParameterValidator;
class StructValidator {
    static for(struct) {
        const receiver = {
            name: struct.name.slice(0, 1).toLowerCase(),
            type: `*${struct.name}`,
        };
        const fieldValidations = new Map();
        for (const prop of struct.properties) {
            const expr = `${receiver.name}.${prop.name}`;
            const descr = `@{desc()}.${prop.name}`;
            const validations = new Array();
            if (!prop.property.optional) {
                validations.push(Validation.nullCheck(expr, descr, prop.reference));
            }
            const validation = Validation.forTypeMap(expr, descr, prop.reference.typeMap);
            if (validation) {
                validations.push(validation);
            }
            if (validations.length > 0) {
                fieldValidations.set(prop, validations);
            }
        }
        if (fieldValidations.size === 0) {
            return undefined;
        }
        return new StructValidator(receiver, fieldValidations);
    }
    constructor(receiver, validations) {
        this.receiver = receiver;
        this.validations = validations;
    }
    get dependencies() {
        return Array.from(this.validations.values()).flatMap((vs) => vs.flatMap((v) => v.dependencies));
    }
    get specialDependencies() {
        return (0, dependencies_1.reduceSpecialDependencies)({
            fmt: true,
            init: false,
            internal: false,
            runtime: false,
            time: false,
        }, ...Array.from(this.validations.values()).flatMap((vs) => vs.flatMap((v) => v.specialDependencies)));
    }
    emitImplementation(code, scope, noOp = false) {
        code.openBlock(`func (${this.receiver.name} ${this.receiver.type}) validate(desc func() string) error`);
        if (noOp) {
            code.line('return nil');
        }
        else {
            for (const [_prop, validations] of this.validations) {
                for (const validation of validations) {
                    validation.emit(code, scope);
                }
                code.line();
            }
            code.line('return nil');
        }
        code.closeBlock();
        code.line();
    }
}
exports.StructValidator = StructValidator;
class Validation {
    static forTypeMap(expression, description, typeMap) {
        switch (typeMap.type) {
            case 'union':
                return Validation.unionCheck(expression, description, typeMap.value);
            case 'interface':
                return Validation.interfaceCheck(expression, description, typeMap.value);
            case 'array':
            case 'map':
                return Validation.collectionCheck(expression, description, typeMap.value);
            case 'primitive':
            case 'void':
                return undefined;
        }
    }
    static collectionCheck(expression, description, elementType) {
        // We need to come up with a unique-enough ID here... so we use a hash.
        const idx = `idx_${(0, crypto_1.createHash)('sha256')
            .update(expression)
            .digest('hex')
            .slice(0, 6)}`;
        // This is actually unused
        const elementValidator = Validation.forTypeMap('v', `${description}[@{${idx}:#v}]`, elementType.typeMap);
        if (elementValidator == null) {
            return undefined;
        }
        class CollectionCheck extends Validation {
            get specialDependencies() {
                return elementValidator.specialDependencies;
            }
            get dependencies() {
                return elementValidator.dependencies;
            }
            emit(code, scope) {
                // We need to de-reference the pointer here (range does not operate on pointers)
                code.openBlock(`for ${idx}, v := range *${expression}`);
                elementValidator.emit(code, scope);
                code.closeBlock();
            }
        }
        return new CollectionCheck();
    }
    static interfaceCheck(expression, description, iface) {
        if (!iface.datatype) {
            return undefined;
        }
        class InterfaceCheck extends Validation {
            get dependencies() {
                return [];
            }
            get specialDependencies() {
                return {
                    fmt: INTERPOLATION.test(description),
                    init: false,
                    internal: false,
                    runtime: !!iface.datatype,
                    time: false,
                };
            }
            emit(code, _scope) {
                code.openBlock(`if err := ${constants_1.JSII_RT_ALIAS}.ValidateStruct(${expression}, func() string { return ${interpolated(description)} }); err != nil`);
                code.line(`return err`);
                code.closeBlock();
            }
        }
        return new InterfaceCheck();
    }
    static nullCheck(expression, description, typeRef) {
        class NullCheck extends Validation {
            get dependencies() {
                return [];
            }
            get specialDependencies() {
                return {
                    fmt: true,
                    init: false,
                    internal: false,
                    runtime: false,
                    time: false,
                };
            }
            emit(code) {
                const nullValue = typeRef.type?.type?.isEnumType()
                    ? `""` // Enums are represented as string-valued constants
                    : 'nil';
                code.openBlock(`if ${expression} == ${nullValue}`);
                code.line(returnErrorf(`${description} is required, but nil was provided`));
                code.closeBlock();
            }
        }
        return new NullCheck();
    }
    static unionCheck(expression, description, types) {
        const hasInterface = types.some((t) => t.typeMap.type === 'interface');
        class UnionCheck extends Validation {
            get dependencies() {
                return types.flatMap((t) => t.dependencies);
            }
            get specialDependencies() {
                return (0, dependencies_1.reduceSpecialDependencies)({
                    fmt: true,
                    init: false,
                    internal: false,
                    runtime: hasInterface,
                    time: false,
                }, ...types.flatMap((t) => {
                    const validator = Validation.forTypeMap(expression, description, t.typeMap);
                    if (validator == null)
                        return [];
                    return [validator.specialDependencies];
                }));
            }
            emit(code, scope) {
                const validTypes = new Array();
                code.line(`switch ${expression}.(type) {`);
                for (const type of types) {
                    const typeName = type.scopedReference(scope);
                    validTypes.push(typeName);
                    // Maps a type to the conversion instructions to the ${typeName} type
                    const acceptableTypes = new Map();
                    acceptableTypes.set(typeName, undefined);
                    switch (typeName) {
                        case '*float64':
                            // For numbers, we accept everything that implictly converts to float64 (pointer & not)
                            acceptableTypes.set('float64', (code, inVar, outVar) => code.line(`${outVar} := &${inVar}`));
                            const ALTERNATE_TYPES = [
                                'int',
                                'uint',
                                'int8',
                                'int16',
                                'int32',
                                'int64',
                                'uint8',
                                'uint16',
                                'uint32',
                                'uint64',
                            ];
                            for (const otherType of ALTERNATE_TYPES) {
                                const varName = (0, crypto_1.createHash)('sha256')
                                    .update(expression)
                                    .digest('hex')
                                    .slice(6);
                                acceptableTypes.set(`*${otherType}`, (code) => {
                                    code.openBlock(`${varName} := func (v *${otherType}) *float64`);
                                    code.openBlock('if v == nil {');
                                    code.line('return nil');
                                    code.closeBlock();
                                    code.line(`val := float64(*v)`);
                                    code.line(`return &val`);
                                    code.closeBlock('()');
                                });
                                acceptableTypes.set(otherType, (code) => {
                                    code.openBlock(`${varName} := func (v ${otherType}) *float64`);
                                    code.line(`val := float64(v)`);
                                    code.line(`return &val`);
                                    code.closeBlock('()');
                                });
                            }
                            break;
                        default:
                            // Accept pointer and non-pointer versions of everything
                            if (typeName.startsWith('*')) {
                                const nonPointerType = typeName.slice(1);
                                acceptableTypes.set(nonPointerType, (code, inVar, outVar) => code.line(`${outVar} := &${inVar}`));
                            }
                    }
                    for (const [acceptableType, conversion] of acceptableTypes) {
                        code.indent(`case ${acceptableType}:`);
                        const outVar = /^[a-z0-9_]+$/.test(expression) ? expression : `v`;
                        const validation = Validation.forTypeMap(outVar, description, type.typeMap);
                        if (validation) {
                            const inVar = conversion ? `${outVar}_` : outVar;
                            code.line(`${inVar} := ${expression}.(${acceptableType})`);
                            if (conversion) {
                                conversion(code, inVar, outVar);
                            }
                            validation.emit(code, scope);
                        }
                        else {
                            code.line('// ok');
                        }
                        code.unindent(false);
                    }
                }
                code.indent('default:');
                if (hasInterface)
                    code.openBlock(`if !${constants_1.JSII_RT_ALIAS}.IsAnonymousProxy(${expression})`);
                code.line(returnErrorf(`${description} must be one of the allowed types: ${validTypes.join(', ')}; received @{${expression}:#v} (a @{${expression}:T})`));
                if (hasInterface)
                    code.closeBlock();
                code.unindent('}');
            }
        }
        return new UnionCheck();
    }
    constructor() { }
}
const INTERPOLATION = /@\{([^}:]+)(?::([^}]+))?\}/;
function interpolated(message) {
    // Need to escape literal percent signes, as a precaution.
    let escaped = message.replace(/%/gm, '%%');
    const args = new Array();
    let match;
    while ((match = INTERPOLATION.exec(escaped))) {
        const before = escaped.slice(0, match.index);
        const expr = match[1];
        const mod = match[2];
        const after = escaped.slice(match.index + match[1].length + 3 + (mod ? mod.length + 1 : 0));
        escaped = `${before}%${mod || 'v'}${after}`;
        args.push(expr);
    }
    if (args.length === 0) {
        return JSON.stringify(message);
    }
    return `fmt.Sprintf(${JSON.stringify(escaped)}, ${args.join(', ')})`;
}
function returnErrorf(message) {
    const args = new Array();
    // Need to escape literal percent signes, as a precaution.
    message = message.replace(/%/gm, '%%');
    let match;
    while ((match = INTERPOLATION.exec(message))) {
        const before = message.slice(0, match.index);
        const expr = match[1];
        const mod = match[2];
        const after = message.slice(match.index + match[1].length + 3 + (mod ? mod.length + 1 : 0));
        message = `${before}%${mod || 'v'}${after}`;
        args.push(expr);
    }
    return `return fmt.Errorf(${[JSON.stringify(message), ...args].join(', ')})`;
}
function syntheticParameter(parent, name, type, optional) {
    return new types_1.GoParameter(parent, new jsii_reflect_1.Parameter(parent.type.system, parent.type, new jsii_reflect_1.Method(parent.type.system, parent.type.assembly, parent.type, parent.type, { name: '__synthetic__' }), {
        name,
        optional,
        type,
    }));
}
//# sourceMappingURL=runtime-type-checking.js.map