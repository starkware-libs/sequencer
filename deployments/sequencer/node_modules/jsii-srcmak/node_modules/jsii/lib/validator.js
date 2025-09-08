"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Validator = void 0;
const assert = require("node:assert");
const spec = require("@jsii/spec");
const deepEqual = require("fast-deep-equal");
const ts = require("typescript");
const Case = require("./case");
const jsii_diagnostic_1 = require("./jsii-diagnostic");
const node_bindings_1 = require("./node-bindings");
const bindings = require("./node-bindings");
class Validator {
    constructor(projectInfo, assembly) {
        this.projectInfo = projectInfo;
        this.assembly = assembly;
    }
    emit() {
        const diagnostics = new Array();
        for (const validation of Validator.VALIDATIONS) {
            validation(this, this.assembly, diagnostics.push.bind(diagnostics));
        }
        return {
            diagnostics: diagnostics,
            emitSkipped: diagnostics.some((diag) => diag.category === ts.DiagnosticCategory.Error),
        };
    }
}
exports.Validator = Validator;
Validator.VALIDATIONS = _defaultValidations();
function _defaultValidations() {
    return [
        _enumMembersMustUserUpperSnakeCase,
        _memberNamesMustUseCamelCase,
        _staticConstantNamesMustUseUpperSnakeCase,
        _memberNamesMustNotLookLikeJavaGettersOrSetters,
        _allTypeReferencesAreValid,
        _inehritanceDoesNotChangeContracts,
        _staticMembersAndNestedTypesMustNotSharePascalCaseName,
        _abstractClassesMustImplementAllProperties,
    ];
    function _enumMembersMustUserUpperSnakeCase(_, assembly, diagnostic) {
        for (const type of _allTypes(assembly)) {
            if (!spec.isEnumType(type)) {
                continue;
            }
            for (const member of type.members) {
                if (member.name && !isConstantCase(member.name)) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_8001_ALL_CAPS_ENUM_MEMBERS.createDetached(member.name, type.fqn));
                }
            }
        }
    }
    function _memberNamesMustUseCamelCase(_, assembly, diagnostic) {
        for (const { member, type } of _allMembers(assembly)) {
            if (member.static && member.const) {
                continue;
            }
            if (member.name && member.name !== Case.camel(member.name)) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_8002_CAMEL_CASED_MEMBERS.createDetached(member.name, type.fqn));
            }
        }
    }
    function _staticConstantNamesMustUseUpperSnakeCase(_, assembly, diagnostic) {
        for (const { member, type } of _allMembers(assembly)) {
            if (!member.static || !member.const) {
                continue;
            }
            if (member.name &&
                !isConstantCase(member.name) &&
                member.name !== Case.pascal(member.name) &&
                member.name !== Case.camel(member.name)) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_8003_STATIC_CONST_CASING.createDetached(member.name, type.name));
            }
        }
    }
    function _memberNamesMustNotLookLikeJavaGettersOrSetters(_, assembly, diagnostic) {
        for (const { member, type } of _allMembers(assembly)) {
            if (!member.name) {
                continue;
            }
            const snakeName = Case.snake(member.name);
            if (snakeName.startsWith('get_') && _isEmpty(member.parameters)) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5000_JAVA_GETTERS.createDetached(member.name, type.name));
            }
            else if (snakeName.startsWith('set_') && (member.parameters ?? []).length === 1) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5001_JAVA_SETTERS.createDetached(member.name, type.name));
            }
        }
    }
    function _allTypeReferencesAreValid(validator, assembly, diagnostic) {
        for (const typeRef of _allTypeReferences(assembly)) {
            const [assm] = typeRef.fqn.split('.');
            if (assembly.name === assm) {
                if (!(typeRef.fqn in (assembly.types ?? {}))) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_3000_EXPORTED_API_USES_HIDDEN_TYPE.create(typeRef.node, // Pretend there is always a value
                    typeRef.fqn));
                }
                continue;
            }
            const foreignAssm = validator.projectInfo.dependencyClosure.find((dep) => dep.name === assm);
            if (!foreignAssm) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_9000_UNKNOWN_MODULE.createDetached(assm));
                continue;
            }
            if (!(typeRef.fqn in (foreignAssm.types ?? {}))) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_9001_TYPE_NOT_FOUND.createDetached(typeRef));
            }
        }
    }
    function _inehritanceDoesNotChangeContracts(validator, assembly, diagnostic) {
        for (const type of _allTypes(assembly)) {
            if (spec.isClassType(type)) {
                for (const method of type.methods ?? []) {
                    _validateMethodOverride(method, type);
                }
                for (const property of type.properties ?? []) {
                    _validatePropertyOverride(property, type);
                }
            }
            if (spec.isClassOrInterfaceType(type) && (type.interfaces?.length ?? 0) > 0) {
                for (const method of _allImplementations(type, (t) => t.methods)) {
                    _validateMethodImplementation(method, type);
                }
                for (const property of _allImplementations(type, (t) => t.properties)) {
                    _validatePropertyImplementation(property, type);
                }
            }
        }
        /**
         * Lists all "implementations" from the given type, using the provided
         * implementation getter. Note that abstract members may be part of the
         * result (in particular, if `type` is an interface type, or if it's an
         * abstract class with unimplemented members) -- I just couldn't come up
         * with a name that actually describes this.
         *
         * @param type   the type which implemented members are needed.
         * @param getter the getter to obtain methods or properties from the type.
         *
         * @returns a list of members (possibly empty, always defined)
         */
        function _allImplementations(type, getter) {
            const result = new Array();
            const known = new Set();
            for (const member of getter(type) ?? []) {
                result.push(member);
                known.add(member.name);
            }
            if (spec.isClassType(type) && type.base) {
                // We have a parent class, collect their concrete members, too (recursively)...
                const base = _dereference(type.base, assembly, validator);
                assert(base != null && spec.isClassType(base));
                for (const member of _allImplementations(base, getter)) {
                    if (known.has(member.name)) {
                        continue;
                    }
                    // The member is copied, so that its `overrides` property won't be
                    // altered, since this member is "borrowed" from a parent type. We
                    // only check it, but should not record `overrides` relationships to
                    // it as those could be invalid per the parent type (i.e: the parent
                    // member may not be able to implement an interface, if that type does
                    // not actually declare implementing that).
                    const memberCopy = { ...member };
                    // Forward the related node if there's one, so diagnostics are bound.
                    const node = bindings.getRelatedNode(member);
                    if (node != null) {
                        bindings.setRelatedNode(memberCopy, node);
                    }
                    result.push(memberCopy);
                    known.add(member.name);
                }
            }
            return result;
        }
        function _validateMethodOverride(method, type) {
            if (!type.base) {
                return false;
            }
            const baseType = _dereference(type.base, assembly, validator);
            if (!baseType) {
                return false;
            }
            const overridden = (baseType.methods ?? []).find((m) => m.name === method.name);
            if (!overridden) {
                return _validateMethodOverride(method, baseType);
            }
            _assertSignaturesMatch(overridden, method, `${type.fqn}#${method.name}`, `overriding ${baseType.fqn}`);
            method.overrides = baseType.fqn;
            return true;
        }
        function _validatePropertyOverride(property, type) {
            if (!type.base) {
                return false;
            }
            const baseType = _dereference(type.base, assembly, validator);
            if (!baseType) {
                return false;
            }
            const overridden = (baseType.properties ?? []).find((p) => p.name === property.name);
            if (!overridden) {
                return _validatePropertyOverride(property, baseType);
            }
            _assertPropertiesMatch(overridden, property, `${type.fqn}#${property.name}`, `overriding ${baseType.fqn}`);
            property.overrides = baseType.fqn;
            return true;
        }
        function _validateMethodImplementation(method, type) {
            if (!type.interfaces) {
                // Abstract classes may not directly implement all members, need to check their supertypes...
                if (spec.isClassType(type) && type.base && type.abstract) {
                    return _validateMethodImplementation(method, _dereference(type.base, assembly, validator));
                }
                return false;
            }
            for (const iface of type.interfaces) {
                const ifaceType = _dereference(iface, assembly, validator);
                const implemented = (ifaceType.methods ?? []).find((m) => m.name === method.name);
                if (implemented) {
                    _assertSignaturesMatch(implemented, method, `${type.fqn}#${method.name}`, `implementing ${ifaceType.fqn}`);
                    // We won't replace a previous overrides declaration from a method override, as those have
                    // higher precedence than an initial implementation.
                    method.overrides = method.overrides ?? iface;
                    return true;
                }
                if (_validateMethodImplementation(method, ifaceType)) {
                    return true;
                }
            }
            return false;
        }
        function _validatePropertyImplementation(property, type) {
            if (!type.interfaces) {
                // Abstract classes may not directly implement all members, need to check their supertypes...
                if (spec.isClassType(type) && type.base && type.abstract) {
                    return _validatePropertyImplementation(property, _dereference(type.base, assembly, validator));
                }
                return false;
            }
            for (const iface of type.interfaces) {
                const ifaceType = _dereference(iface, assembly, validator);
                const implemented = (ifaceType.properties ?? []).find((p) => p.name === property.name);
                if (implemented) {
                    _assertPropertiesMatch(implemented, property, `${type.fqn}#${property.name}`, `implementing ${ifaceType.fqn}`);
                    // We won't replace a previous overrides declaration from a property override, as those
                    // have higher precedence than an initial implementation.
                    property.overrides = property.overrides ?? ifaceType.fqn;
                    return true;
                }
                if (_validatePropertyImplementation(property, ifaceType)) {
                    return true;
                }
            }
            return false;
        }
        function _assertSignaturesMatch(expected, actual, label, action) {
            if (!!expected.protected !== !!actual.protected) {
                const expVisibility = expected.protected ? 'protected' : 'public';
                const actVisibility = actual.protected ? 'protected' : 'public';
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5002_OVERRIDE_CHANGES_VISIBILITY.createDetached(label, action, actVisibility, expVisibility));
            }
            if (!deepEqual(actual.returns, expected.returns)) {
                const expType = spec.describeTypeReference(expected.returns?.type);
                const actType = spec.describeTypeReference(actual.returns?.type);
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5003_OVERRIDE_CHANGES_RETURN_TYPE.createDetached(label, action, actType, expType));
            }
            const expectedParams = expected.parameters ?? [];
            const actualParams = actual.parameters ?? [];
            if (expectedParams.length !== actualParams.length) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5005_OVERRIDE_CHANGES_PARAM_COUNT.createDetached(label, action, actualParams.length, expectedParams.length));
                return;
            }
            for (let i = 0; i < expectedParams.length; i++) {
                const expParam = expectedParams[i];
                const actParam = actualParams[i];
                if (!deepEqual(expParam.type, actParam.type)) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5006_OVERRIDE_CHANGES_PARAM_TYPE.createDetached(label, action, actParam, expParam));
                }
                // Not-ing those to force the values to a strictly boolean context (they're optional, undefined means false)
                if (expParam.variadic !== actParam.variadic) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5007_OVERRIDE_CHANGES_VARIADIC.createDetached(label, action, actParam.variadic, expParam.variadic));
                }
                if (expParam.optional !== actParam.optional) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5008_OVERRIDE_CHANGES_PARAM_OPTIONAL.createDetached(label, action, actParam, expParam));
                }
            }
        }
        function _assertPropertiesMatch(expected, actual, label, action) {
            const actualNode = bindings.getPropertyRelatedNode(actual);
            const expectedNode = bindings.getPropertyRelatedNode(expected);
            if (!!expected.protected !== !!actual.protected) {
                const expVisibility = expected.protected ? 'protected' : 'public';
                const actVisibility = actual.protected ? 'protected' : 'public';
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5002_OVERRIDE_CHANGES_VISIBILITY.create(actualNode?.modifiers?.find((mod) => mod.kind === ts.SyntaxKind.PublicKeyword || mod.kind === ts.SyntaxKind.ProtectedKeyword) ?? declarationName(actualNode), label, action, actVisibility, expVisibility).maybeAddRelatedInformation(expectedNode?.modifiers?.find((mod) => mod.kind === ts.SyntaxKind.PublicKeyword || mod.kind === ts.SyntaxKind.ProtectedKeyword) ?? declarationName(expectedNode), 'The implemented declaration is here.'));
            }
            if (!deepEqual(expected.type, actual.type)) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5004_OVERRIDE_CHANGES_PROP_TYPE.create(actualNode?.type ?? declarationName(actualNode), label, action, actual.type, expected.type).maybeAddRelatedInformation(expectedNode?.type ?? declarationName(expectedNode), 'The implemented declaration is here.'));
            }
            if (expected.immutable !== actual.immutable) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5010_OVERRIDE_CHANGES_MUTABILITY.create(actualNode?.modifiers?.find((mod) => mod.kind === ts.SyntaxKind.ReadonlyKeyword) ??
                    declarationName(actualNode), label, action, actual.immutable, expected.immutable).maybeAddRelatedInformation(expectedNode?.modifiers?.find((mod) => mod.kind === ts.SyntaxKind.ReadonlyKeyword) ??
                    declarationName(expectedNode), 'The implemented declaration is here.'));
            }
            if (expected.optional !== actual.optional) {
                diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5009_OVERRIDE_CHANGES_PROP_OPTIONAL.create(actualNode?.questionToken ?? actualNode?.type ?? declarationName(actualNode), label, action, actual.optional, expected.optional).maybeAddRelatedInformation(expectedNode?.questionToken ?? expectedNode?.type ?? declarationName(expectedNode), 'The implemented declaration is here.'));
            }
        }
    }
    /**
     * Abstract classes that implement an interface should have a declaration for every member.
     *
     * For non-optional members, TypeScript already enforces this. This leaves the user the
     * ability to forget optional properties (`readonly prop?: string`).
     *
     * At least our codegen for this case fails in C#, and I'm not convinced it does the right
     * thing in Java either. So we will disallow this, and require users to declare these
     * fields on the class. It can always be `public abstract readonly prop?: string` if they
     * don't want to give an implementation yet.
     */
    function _abstractClassesMustImplementAllProperties(validator, assembly, diagnostic) {
        for (const type of _allTypes(assembly)) {
            if (!spec.isClassType(type) || !type.abstract) {
                continue;
            }
            const classProps = collectClassProps(type, new Set());
            for (const implFqn of type.interfaces ?? []) {
                checkInterfacePropsImplemented(implFqn, type, classProps);
            }
        }
        /**
         * Return all property names declared on this class and its base classes
         */
        function collectClassProps(type, into) {
            for (const prop of type.properties ?? []) {
                into.add(prop.name);
            }
            if (type.base) {
                const base = _dereference(type.base, assembly, validator);
                if (spec.isClassType(base)) {
                    collectClassProps(base, into);
                }
            }
            return into;
        }
        function checkInterfacePropsImplemented(interfaceFqn, cls, propNames) {
            const intf = _dereference(interfaceFqn, assembly, validator);
            if (!spec.isInterfaceType(intf)) {
                return;
            }
            // We only have to check for optional properties, because anything required
            // would have been caught by the TypeScript compiler already.
            for (const prop of intf.properties ?? []) {
                if (!prop.optional) {
                    continue;
                }
                if (!propNames.has(prop.name)) {
                    diagnostic(jsii_diagnostic_1.JsiiDiagnostic.JSII_5021_ABSTRACT_CLASS_MISSING_PROP_IMPL.create(bindings.getClassOrInterfaceRelatedNode(cls), intf, cls, prop.name).maybeAddRelatedInformation(bindings.getPropertyRelatedNode(prop), 'The implemented declaration is here.'));
                }
            }
            for (const extFqn of intf.interfaces ?? []) {
                checkInterfacePropsImplemented(extFqn, cls, propNames);
            }
        }
    }
    function _staticMembersAndNestedTypesMustNotSharePascalCaseName(_, assembly, diagnostic) {
        for (const nestedType of Object.values(assembly.types ?? {})) {
            if (nestedType.namespace == null) {
                continue;
            }
            const nestingType = assembly.types[`${assembly.name}.${nestedType.namespace}`];
            if (nestingType == null) {
                continue;
            }
            const nestedTypeName = Case.pascal(nestedType.name);
            for (const { name, member } of staticMembers(nestingType)) {
                if (name === nestedTypeName) {
                    let diag = jsii_diagnostic_1.JsiiDiagnostic.JSII_5020_STATIC_MEMBER_CONFLICTS_WITH_NESTED_TYPE.create((0, node_bindings_1.getRelatedNode)(member), nestingType, member, nestedType);
                    const nestedTypeNode = (0, node_bindings_1.getRelatedNode)(nestedType);
                    if (nestedTypeNode != null) {
                        diag = diag.addRelatedInformation(nestedTypeNode, 'This is the conflicting nested type declaration');
                    }
                    diagnostic(diag);
                }
            }
        }
        function staticMembers(type) {
            if (spec.isClassOrInterfaceType(type)) {
                return [
                    ...(type.methods?.filter((method) => method.static) ?? []),
                    ...(type.properties?.filter((prop) => prop.static) ?? []),
                ].map((member) => ({ name: Case.pascal(member.name), member }));
            }
            return type.members.map((member) => ({ name: member.name, member }));
        }
    }
}
function _allTypes(assm) {
    return Object.values(assm.types ?? {});
}
function _allMethods(assm) {
    const methods = new Array();
    for (const type of _allTypes(assm)) {
        if (!spec.isClassOrInterfaceType(type)) {
            continue;
        }
        if (!type.methods) {
            continue;
        }
        for (const method of type.methods)
            methods.push({ member: method, type });
    }
    return methods;
}
function _allProperties(assm) {
    const properties = new Array();
    for (const type of _allTypes(assm)) {
        if (!spec.isClassOrInterfaceType(type)) {
            continue;
        }
        if (!type.properties) {
            continue;
        }
        for (const property of type.properties)
            properties.push({ member: property, type });
    }
    return properties;
}
function _allMembers(assm) {
    return [..._allMethods(assm), ..._allProperties(assm)];
}
function _allTypeReferences(assm) {
    const typeReferences = new Array();
    for (const type of _allTypes(assm)) {
        if (!spec.isClassOrInterfaceType(type)) {
            continue;
        }
        if (spec.isClassType(type)) {
            const node = bindings.getClassRelatedNode(type);
            if (type.base) {
                typeReferences.push({
                    fqn: type.base,
                    node: node?.heritageClauses?.find((hc) => hc.token === ts.SyntaxKind.ExtendsKeyword)?.types[0],
                });
            }
            if (type.initializer?.parameters) {
                for (const param of type.initializer.parameters) {
                    _collectTypeReferences(param.type, bindings.getParameterRelatedNode(param)?.type);
                }
            }
        }
        if (type.interfaces) {
            const node = bindings.getClassOrInterfaceRelatedNode(type);
            for (const iface of type.interfaces) {
                typeReferences.push({
                    fqn: iface,
                    node: node?.heritageClauses?.find((hc) => hc.token ===
                        (spec.isInterfaceType(type) ? ts.SyntaxKind.ImplementsKeyword : ts.SyntaxKind.ExtendsKeyword)),
                });
            }
        }
    }
    for (const { member: prop } of _allProperties(assm)) {
        _collectTypeReferences(prop.type, bindings.getPropertyRelatedNode(prop)?.type);
    }
    for (const { member: meth } of _allMethods(assm)) {
        if (meth.returns) {
            _collectTypeReferences(meth.returns.type, bindings.getMethodRelatedNode(meth)?.type);
        }
        for (const param of meth.parameters ?? []) {
            _collectTypeReferences(param.type, bindings.getParameterRelatedNode(param)?.type);
        }
    }
    return typeReferences;
    function _collectTypeReferences(type, node) {
        if (spec.isNamedTypeReference(type)) {
            typeReferences.push({ ...type, node });
        }
        else if (spec.isCollectionTypeReference(type)) {
            _collectTypeReferences(type.collection.elementtype, node);
        }
        else if (spec.isUnionTypeReference(type)) {
            for (const t of type.union.types)
                _collectTypeReferences(t, node);
        }
    }
}
function _dereference(typeRef, assembly, validator) {
    if (typeof typeRef !== 'string') {
        typeRef = typeRef.fqn;
    }
    const [assm] = typeRef.split('.');
    if (assembly.name === assm) {
        return assembly.types?.[typeRef];
    }
    const foreignAssm = validator.projectInfo.dependencyClosure.find((dep) => dep.name === assm);
    return foreignAssm?.types?.[typeRef];
}
function _isEmpty(array) {
    return array == null || array.length === 0;
}
/**
 * Return whether an identifier only consists of upperchase characters, digits and underscores
 *
 * We have our own check here (isConstantCase) which is more lenient than what
 * `case.constant()` prescribes. We also want to allow combinations of letters
 * and digits without underscores: `C5A`, which `case` would force to `C5_A`.
 * The hint we print will still use `case.constant()` but that is fine.
 */
function isConstantCase(x) {
    return !/[^A-Z0-9_]/.exec(x);
}
/**
 * Obtains the name of the given declaration, if it has one, or returns the declaration itself.
 * This function is meant to be used as a convenience to obtain the `ts.Node` to bind a
 * `JsiiDianostic` instance on.
 *
 * It may return `undefined` but is typed as `ts.Node` so that it is easier to use with
 * `JsiiDiagnostic` factories.
 *
 * @param decl the declaration which name is needed.
 *
 * @returns the name of the declaration if it has one, or the declaration itself. Might return
 *          `undefined` if the provided declaration is undefined.
 */
function declarationName(decl) {
    if (decl == null) {
        // Pretend we returned a node - this is used to create diagnostics, worst case it'll be unbound.
        return decl;
    }
    return ts.getNameOfDeclaration(decl) ?? decl;
}
//# sourceMappingURL=validator.js.map