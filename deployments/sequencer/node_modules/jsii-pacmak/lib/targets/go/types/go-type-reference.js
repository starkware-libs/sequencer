"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.GoTypeRef = void 0;
const log = require("../../../logging");
/*
 * Maps names of JS primitives to corresponding Go types as strings
 */
class PrimitiveMapper {
    constructor(name) {
        this.name = name;
        this.MAP = {
            number: 'float64',
            boolean: 'bool',
            any: 'interface{}',
            date: 'time.Time',
            string: 'string',
            json: `map[string]interface{}`,
        };
    }
    get goPrimitive() {
        const val = this.MAP[this.name];
        if (!val) {
            log.debug(`Unmapped primitive type: ${this.name}`);
        }
        return val ?? this.name;
    }
}
/*
 * Accepts a JSII TypeReference and Go Package and can resolve the GoType within the module tree.
 */
class GoTypeRef {
    constructor(root, reference, options = {
        opaqueUnionTypes: true,
    }) {
        this.root = root;
        this.reference = reference;
        this.options = options;
    }
    get type() {
        if (this.reference.fqn) {
            return this.root.findType(this.reference.fqn);
        }
        return undefined;
    }
    get specialDependencies() {
        return {
            fmt: false,
            init: false,
            internal: false,
            runtime: false,
            time: containsDate(this.reference, this.options.opaqueUnionTypes),
        };
        function containsDate(ref, opaqueUnionType) {
            if (ref.primitive === 'date') {
                return true;
            }
            if (ref.arrayOfType) {
                return containsDate(ref.arrayOfType, opaqueUnionType);
            }
            if (ref.mapOfType) {
                return containsDate(ref.mapOfType, opaqueUnionType);
            }
            if (!opaqueUnionType && ref.unionOfTypes) {
                return ref.unionOfTypes.some((item) => containsDate(item, opaqueUnionType));
            }
            return false;
        }
    }
    get primitiveType() {
        if (this.reference.primitive) {
            return new PrimitiveMapper(this.reference.primitive).goPrimitive;
        }
        return undefined;
    }
    get name() {
        return this.type?.name;
    }
    get datatype() {
        const reflectType = this.type?.type;
        return reflectType?.isInterfaceType() && reflectType.datatype;
    }
    get namespace() {
        return this.type?.namespace;
    }
    get void() {
        return this.reference.void;
    }
    get typeMap() {
        this._typeMap ?? (this._typeMap = this.buildTypeMap(this));
        return this._typeMap;
    }
    /**
     * The go `import`s required in order to be able to use this type in code.
     */
    get dependencies() {
        const ret = new Array();
        switch (this.typeMap.type) {
            case 'interface':
                if (this.type?.pkg) {
                    ret.push(this.type.pkg);
                }
                break;
            case 'array':
            case 'map':
                ret.push(...this.typeMap.value.dependencies);
                break;
            case 'union':
                if (!this.options.opaqueUnionTypes) {
                    for (const t of this.typeMap.value) {
                        ret.push(...t.dependencies);
                    }
                }
                break;
            case 'void':
            case 'primitive':
                break;
        }
        return ret;
    }
    get unionOfTypes() {
        const typeMap = this.typeMap;
        if (typeMap.type !== 'union') {
            return undefined;
        }
        return typeMap.value;
    }
    get withTransparentUnions() {
        if (!this.options.opaqueUnionTypes) {
            return this;
        }
        return new GoTypeRef(this.root, this.reference, {
            ...this.options,
            opaqueUnionTypes: false,
        });
    }
    /*
     * Return the name of a type for reference from the `Package` passed in
     */
    scopedName(scope) {
        return this.scopedTypeName(this.typeMap, scope);
    }
    scopedReference(scope) {
        return this.scopedTypeName(this.typeMap, scope, true);
    }
    buildTypeMap(ref) {
        if (ref.primitiveType) {
            return { type: 'primitive', value: ref.primitiveType };
        }
        else if (ref.reference.arrayOfType) {
            return {
                type: 'array',
                value: new GoTypeRef(this.root, ref.reference.arrayOfType, this.options),
            };
        }
        else if (ref.reference.mapOfType) {
            return {
                type: 'map',
                value: new GoTypeRef(this.root, ref.reference.mapOfType, this.options),
            };
        }
        else if (ref.reference.unionOfTypes) {
            return {
                type: 'union',
                value: ref.reference.unionOfTypes.map((typeRef) => new GoTypeRef(this.root, typeRef, this.options)),
            };
        }
        else if (ref.reference.void) {
            return { type: 'void' };
        }
        return { type: 'interface', value: ref };
    }
    scopedTypeName(typeMap, scope, asRef = false) {
        if (typeMap.type === 'primitive') {
            const { value } = typeMap;
            const prefix = asRef && value !== 'interface{}' ? '*' : '';
            return `${prefix}${value}`;
        }
        else if (typeMap.type === 'array' || typeMap.type === 'map') {
            const prefix = asRef ? '*' : '';
            const wrapper = typeMap.type === 'array' ? '[]' : 'map[string]';
            const innerName = this.scopedTypeName(typeMap.value.typeMap, scope, asRef) ??
                'interface{}';
            return `${prefix}${wrapper}${innerName}`;
        }
        else if (typeMap.type === 'interface') {
            const prefix = asRef && typeMap.value.datatype ? '*' : '';
            const baseName = typeMap.value.name;
            // type is defined in the same scope as the current one, no namespace required
            if (scope.packageName === typeMap.value.namespace && baseName) {
                // if the current scope is the same as the types scope, return without a namespace
                return `${prefix}${baseName}`;
            }
            // type is defined in another module and requires a namespace and import
            if (baseName) {
                return `${prefix}${typeMap.value.namespace}.${baseName}`;
            }
        }
        else if (typeMap.type === 'union') {
            return 'interface{}';
        }
        else if (typeMap.type === 'void') {
            return '';
        }
        // type isn't handled
        throw new Error(`Type ${typeMap.value?.name} does not resolve to a known Go type.`);
    }
}
exports.GoTypeRef = GoTypeRef;
//# sourceMappingURL=go-type-reference.js.map