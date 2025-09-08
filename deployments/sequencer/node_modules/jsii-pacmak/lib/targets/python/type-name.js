"use strict";
var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _Dict_element, _List_element, _Optional_wrapped, _Primitive_pythonType, _Union_options, _UserType_fqn;
Object.defineProperty(exports, "__esModule", { value: true });
exports.toTypeName = toTypeName;
exports.toPackageName = toPackageName;
exports.mergePythonImports = mergePythonImports;
exports.toPythonFqn = toPythonFqn;
const spec_1 = require("@jsii/spec");
const codemaker_1 = require("codemaker");
const crypto_1 = require("crypto");
const util_1 = require("./util");
function toTypeName(ref) {
    if (ref == null) {
        return Primitive.NONE;
    }
    const type = isOptionalValue(ref) ? ref.type : ref;
    const optional = isOptionalValue(ref) && ref.optional;
    let result = Primitive.ANY;
    if ((0, spec_1.isPrimitiveTypeReference)(type)) {
        result = Primitive.of(type);
    }
    else if ((0, spec_1.isCollectionTypeReference)(type)) {
        const elt = toTypeName(type.collection.elementtype);
        if (type.collection.kind === spec_1.CollectionKind.Array) {
            result = new List(elt);
        }
        else {
            result = new Dict(elt);
        }
    }
    else if ((0, spec_1.isUnionTypeReference)(type)) {
        result = new Union(type.union.types.map(toTypeName));
    }
    else if ((0, spec_1.isNamedTypeReference)(type)) {
        result = new UserType(type.fqn);
    }
    return optional ? new Optional(result) : result;
}
/**
 * Obtains the Python package name for a given submodule FQN.
 *
 * @param fqn      the submodule FQN for which a package name is needed.
 * @param rootAssm the assembly this FQN belongs to.
 */
function toPackageName(fqn, rootAssm) {
    return getPackageName(fqn, rootAssm).packageName;
}
function mergePythonImports(...pythonImports) {
    const result = {};
    for (const bag of pythonImports) {
        for (const [packageName, items] of Object.entries(bag)) {
            if (!(packageName in result)) {
                result[packageName] = new Set();
            }
            for (const item of items) {
                result[packageName].add(item);
            }
        }
    }
    return result;
}
function isOptionalValue(type) {
    return type.type != null;
}
class Dict {
    constructor(element) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _Dict_element.set(this, void 0);
        __classPrivateFieldSet(this, _Dict_element, element, "f");
    }
    pythonType(context) {
        return `typing.Mapping[builtins.str, ${__classPrivateFieldGet(this, _Dict_element, "f").pythonType(context)}]`;
    }
    requiredImports(context) {
        return __classPrivateFieldGet(this, _Dict_element, "f").requiredImports(context);
    }
}
_Dict_element = new WeakMap();
class List {
    constructor(element) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _List_element.set(this, void 0);
        __classPrivateFieldSet(this, _List_element, element, "f");
    }
    pythonType(context) {
        const type = context.parameterType ? 'Sequence' : 'List';
        return `typing.${type}[${__classPrivateFieldGet(this, _List_element, "f").pythonType(context)}]`;
    }
    requiredImports(context) {
        return __classPrivateFieldGet(this, _List_element, "f").requiredImports(context);
    }
}
_List_element = new WeakMap();
class Optional {
    constructor(wrapped) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _Optional_wrapped.set(this, void 0);
        __classPrivateFieldSet(this, _Optional_wrapped, wrapped, "f");
    }
    pythonType(context) {
        const optionalType = __classPrivateFieldGet(this, _Optional_wrapped, "f").pythonType({
            ...context,
            ignoreOptional: true,
        });
        // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
        if (context.ignoreOptional || __classPrivateFieldGet(this, _Optional_wrapped, "f") === Primitive.ANY) {
            return optionalType;
        }
        return `typing.Optional[${optionalType}]`;
    }
    requiredImports(context) {
        return __classPrivateFieldGet(this, _Optional_wrapped, "f").requiredImports({ ...context, ignoreOptional: true });
    }
}
_Optional_wrapped = new WeakMap();
class Primitive {
    static of(type) {
        switch (type.primitive) {
            case spec_1.PrimitiveType.Boolean:
                return Primitive.BOOL;
            case spec_1.PrimitiveType.Date:
                return Primitive.DATE;
            case spec_1.PrimitiveType.Number:
                return Primitive.JSII_NUMBER;
            case spec_1.PrimitiveType.String:
                return Primitive.STR;
            case spec_1.PrimitiveType.Json:
                return Primitive.JSON;
            case spec_1.PrimitiveType.Any:
            default:
                return Primitive.ANY;
        }
    }
    constructor(pythonType) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _Primitive_pythonType.set(this, void 0);
        __classPrivateFieldSet(this, _Primitive_pythonType, pythonType, "f");
    }
    pythonType() {
        return __classPrivateFieldGet(this, _Primitive_pythonType, "f");
    }
    requiredImports() {
        return {};
    }
}
_Primitive_pythonType = new WeakMap();
Primitive.BOOL = new Primitive('builtins.bool');
Primitive.DATE = new Primitive('datetime.datetime');
Primitive.JSII_NUMBER = new Primitive('jsii.Number'); // "jsii" is always already imported!
Primitive.STR = new Primitive('builtins.str');
Primitive.JSON = new Primitive('typing.Mapping[typing.Any, typing.Any]');
Primitive.ANY = new Primitive('typing.Any');
Primitive.NONE = new Primitive('None');
class Union {
    constructor(options) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _Union_options.set(this, void 0);
        __classPrivateFieldSet(this, _Union_options, options, "f");
    }
    pythonType(context) {
        return `typing.Union[${__classPrivateFieldGet(this, _Union_options, "f")
            .map((o) => o.pythonType(context))
            .join(', ')}]`;
    }
    requiredImports(context) {
        return mergePythonImports(...__classPrivateFieldGet(this, _Union_options, "f").map((o) => o.requiredImports(context)));
    }
}
_Union_options = new WeakMap();
class UserType {
    constructor(fqn) {
        // eslint-disable-next-line @typescript-eslint/explicit-member-accessibility
        _UserType_fqn.set(this, void 0);
        __classPrivateFieldSet(this, _UserType_fqn, fqn, "f");
    }
    pythonType(context) {
        return this.resolve(context).pythonType;
    }
    requiredImports(context) {
        const requiredImport = this.resolve(context).requiredImport;
        if (requiredImport == null) {
            return {};
        }
        return { [requiredImport.sourcePackage]: new Set([requiredImport.item]) };
    }
    resolve({ assembly, emittedTypes, submodule, surroundingTypeFqns, typeAnnotation = true, parameterType, typeResolver, }) {
        const { assemblyName, packageName, pythonFqn } = toPythonFqn(__classPrivateFieldGet(this, _UserType_fqn, "f"), assembly);
        // If this is a type annotation for a parameter, allow dicts to be passed where structs are expected.
        const type = typeResolver(__classPrivateFieldGet(this, _UserType_fqn, "f"));
        const isStruct = (0, spec_1.isInterfaceType)(type) && !!type.datatype;
        const wrapType = typeAnnotation && parameterType && isStruct
            ? (pyType) => `typing.Union[${pyType}, typing.Dict[builtins.str, typing.Any]]`
            : (pyType) => pyType;
        // Emit aliased imports for dependencies (this avoids name collisions)
        if (assemblyName !== assembly.name) {
            const aliasSuffix = (0, crypto_1.createHash)('sha256')
                .update(assemblyName)
                .update('.*')
                .digest('hex')
                .substring(0, 8);
            const alias = `_${packageName.replace(/\./g, '_')}_${aliasSuffix}`;
            const aliasedFqn = `${alias}${pythonFqn.slice(packageName.length)}`;
            return {
                // If it's a struct, then we allow passing as a dict, too...
                pythonType: wrapType(aliasedFqn),
                requiredImport: {
                    sourcePackage: `${packageName} as ${alias}`,
                    item: '',
                },
            };
        }
        const submodulePythonName = toPythonFqn(submodule, assembly).pythonFqn;
        const typeSubmodulePythonName = toPythonFqn(findParentSubmodule(type, assembly), assembly).pythonFqn;
        if (typeSubmodulePythonName === submodulePythonName) {
            // Identify declarations that are not yet initialized and hence cannot be
            // used as part of a type qualification. Since this is not a forward
            // reference, the type was already emitted and its un-qualified name must
            // be used instead of its locally qualified name.
            const nestingParent = surroundingTypeFqns
                ?.map((fqn) => toPythonFqn(fqn, assembly).pythonFqn)
                ?.reverse()
                ?.find((parent) => pythonFqn.startsWith(`${parent}.`));
            if (typeAnnotation &&
                (!emittedTypes.has(__classPrivateFieldGet(this, _UserType_fqn, "f")) || nestingParent != null)) {
                // Possibly a forward reference, outputting the stringifierd python FQN
                return {
                    pythonType: wrapType(JSON.stringify(pythonFqn.substring(submodulePythonName.length + 1))),
                };
            }
            if (!typeAnnotation && nestingParent) {
                // This is not for a type annotation, so we should be at a point in time
                // where the surrounding symbol has been defined entirely, so we can
                // refer to it "normally" now.
                return { pythonType: pythonFqn.slice(packageName.length + 1) };
            }
            // We'll just make a module-qualified reference at this point.
            return {
                pythonType: wrapType(pythonFqn.substring(submodulePythonName.length + 1)),
            };
        }
        const [toImport, ...nested] = pythonFqn
            .substring(typeSubmodulePythonName.length + 1)
            .split('.');
        const aliasSuffix = (0, crypto_1.createHash)('sha256')
            .update(typeSubmodulePythonName)
            .update('.')
            .update(toImport)
            .digest('hex')
            .substring(0, 8);
        const alias = `_${toImport}_${aliasSuffix}`;
        return {
            pythonType: wrapType([alias, ...nested].join('.')),
            requiredImport: {
                sourcePackage: relativeImportPath(submodulePythonName, typeSubmodulePythonName),
                item: `${toImport} as ${alias}`,
            },
        };
    }
}
_UserType_fqn = new WeakMap();
function toPythonFqn(fqn, rootAssm) {
    const { assemblyName, packageName, tail } = getPackageName(fqn, rootAssm);
    const fqnParts = [packageName];
    for (const part of tail) {
        fqnParts.push((0, util_1.toPythonIdentifier)(part));
    }
    return { assemblyName, packageName, pythonFqn: fqnParts.join('.') };
}
/**
 * Computes the python relative import path from `fromModule` to `toModule`.
 *
 * @param fromPkg the package where the relative import statement is located.
 * @param toPkg   the package that needs to be relatively imported.
 *
 * @returns a relative import path.
 *
 * @example
 *  relativeImportPath('A.B.C.D', 'A.B.E') === '...E';
 *  relativeImportPath('A.B.C', 'A.B')     === '..';
 *  relativeImportPath('A.B', 'A.B.C')     === '.C';
 */
function relativeImportPath(fromPkg, toPkg) {
    if (toPkg.startsWith(fromPkg)) {
        // from A.B to A.B.C === .C
        return `.${toPkg.substring(fromPkg.length + 1)}`;
    }
    // from A.B.E to A.B.C === .<from A.B to A.B.C>
    const fromPkgParent = fromPkg.substring(0, fromPkg.lastIndexOf('.'));
    return `.${relativeImportPath(fromPkgParent, toPkg)}`;
}
function getPackageName(fqn, rootAssm) {
    const segments = fqn.split('.');
    const assemblyName = segments[0];
    const config = assemblyName === rootAssm.name
        ? rootAssm
        : (rootAssm.dependencyClosure?.[assemblyName] ??
            (0, util_1.die)(`Unable to find configuration for assembly "${assemblyName}" in dependency closure`));
    const rootPkg = config.targets?.python?.module ??
        (0, util_1.die)(`No Python target was configured in assembly "${assemblyName}"`);
    const pkg = new Array();
    const tail = new Array();
    for (let len = segments.length; len > 0; len--) {
        const submodule = segments.slice(0, len).join('.');
        if (submodule === assemblyName) {
            pkg.unshift(rootPkg);
            break;
        }
        const submoduleConfig = config.submodules?.[submodule];
        if (submoduleConfig == null) {
            // Not in a submodule - so the current lead name is not a package name part.
            tail.unshift(segments[len - 1]);
            continue;
        }
        const subPackage = submoduleConfig.targets?.python?.module;
        if (subPackage != null) {
            // Found a sub-package. Confirm it's nested right in, and make this the head end of our package name.
            if (!subPackage.startsWith(`${rootPkg}.`)) {
                (0, util_1.die)(`Submodule "${submodule}" is mapped to Python sub-package "${subPackage}" which isn't nested under "${rootPkg}"!`);
            }
            pkg.unshift(subPackage);
            break;
        }
        // Just use whatever the default name is for this package name part.
        pkg.unshift((0, codemaker_1.toSnakeCase)((0, util_1.toPythonIdentifier)(segments[len - 1])));
    }
    return { assemblyName, packageName: pkg.join('.'), tail };
}
function findParentSubmodule(type, assm) {
    if (type.namespace == null) {
        return assm.name;
    }
    const namespaceFqn = `${assm.name}.${type.namespace}`;
    if (assm.types?.[namespaceFqn] != null) {
        return findParentSubmodule(assm.types?.[namespaceFqn], assm);
    }
    return namespaceFqn;
}
//# sourceMappingURL=type-name.js.map