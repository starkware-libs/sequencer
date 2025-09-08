"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Generator = void 0;
const spec = require("@jsii/spec");
const clone = require("clone");
const codemaker_1 = require("codemaker");
const crypto = require("crypto");
const fs = require("fs-extra");
const path = require("path");
const version_1 = require("./version");
/**
 * Abstract base class for jsii package generators.
 * Given a jsii module, it will invoke "events" to emit various elements.
 */
class Generator {
    constructor(options) {
        this.options = options;
        this.excludeTypes = new Array();
        this.code = new codemaker_1.CodeMaker();
    }
    get runtimeTypeChecking() {
        return this.options.runtimeTypeChecking;
    }
    get assembly() {
        if (!this._assembly) {
            throw new Error('No assembly has been loaded! The #load() method must be called first!');
        }
        return this._assembly;
    }
    get reflectAssembly() {
        if (!this._reflectAssembly) {
            throw new Error('Call load() first');
        }
        return this._reflectAssembly;
    }
    get metadata() {
        return { fingerprint: this.fingerprint };
    }
    async load(_packageRoot, assembly) {
        this._reflectAssembly = assembly;
        this._assembly = assembly.spec;
        // Including the version of jsii-pacmak in the fingerprint, as a new version may imply different code generation.
        this.fingerprint = crypto
            .createHash('sha256')
            .update(version_1.VERSION_DESC)
            .update('\0')
            .update(this.assembly.fingerprint)
            .digest('base64');
        return Promise.resolve();
    }
    /**
     * Runs the generator (in-memory).
     */
    generate(fingerprint) {
        this.onBeginAssembly(this.assembly, fingerprint);
        this.visit(spec.NameTree.of(this.assembly));
        this.onEndAssembly(this.assembly, fingerprint);
    }
    async upToDate(_) {
        return Promise.resolve(false);
    }
    /**
     * Returns the file name of the assembly resource as it is going to be saved.
     */
    getAssemblyFileName() {
        let name = this.assembly.name;
        const parts = name.split('/');
        if (parts.length === 1) {
            name = parts[0];
        }
        else if (parts.length === 2 && parts[0].startsWith('@')) {
            name = parts[1];
        }
        else {
            throw new Error('Malformed assembly name. Expecting either <name> or @<scope>/<name>');
        }
        return `${name}@${this.assembly.version}.jsii.tgz`;
    }
    /**
     * Saves all generated files to an output directory, creating any subdirs if needed.
     */
    async save(outdir, tarball, { license, notice }) {
        const assemblyDir = this.getAssemblyOutputDir(this.assembly);
        if (assemblyDir) {
            const fullPath = path.resolve(path.join(outdir, assemblyDir, this.getAssemblyFileName()));
            await fs.mkdirp(path.dirname(fullPath));
            await fs.copy(tarball, fullPath, { overwrite: true });
            if (license) {
                await fs.writeFile(path.resolve(outdir, 'LICENSE'), license, {
                    encoding: 'utf8',
                });
            }
            if (notice) {
                await fs.writeFile(path.resolve(outdir, 'NOTICE'), notice, {
                    encoding: 'utf8',
                });
            }
        }
        return this.code.save(outdir);
    }
    //
    // Bundled assembly
    // jsii modules should bundle the assembly itself as a resource and use the load() kernel API to load it.
    //
    /**
     * Returns the destination directory for the assembly file.
     */
    getAssemblyOutputDir(_mod) {
        return undefined;
    }
    //
    // Assembly
    onBeginAssembly(_assm, _fingerprint) {
        /* noop */
    }
    onEndAssembly(_assm, _fingerprint) {
        /* noop */
    }
    //
    // Namespaces
    onBeginNamespace(_ns) {
        /* noop */
    }
    onEndNamespace(_ns) {
        /* noop */
    }
    //
    // Classes
    onBeginClass(_cls, _abstract) {
        /* noop */
    }
    onEndClass(_cls) {
        /* noop */
    }
    //
    // Initializers (constructos)
    onInitializer(_cls, _initializer) {
        /* noop */
    }
    onInitializerOverload(_cls, _overload, _originalInitializer) {
        /* noop */
    }
    //
    // Properties
    onBeginProperties(_cls) {
        /* noop */
    }
    onEndProperties(_cls) {
        /* noop */
    }
    onExpandedUnionProperty(_cls, _prop, _primaryName) {
        return;
    }
    //
    // Methods
    // onMethodOverload is triggered if the option `generateOverloadsForMethodWithOptionals` is enabled for each overload of the original method.
    // The original method will be emitted via onMethod.
    onBeginMethods(_cls) {
        /* noop */
    }
    onEndMethods(_cls) {
        /* noop */
    }
    //
    // Enums
    onBeginEnum(_enm) {
        /* noop */
    }
    onEndEnum(_enm) {
        /* noop */
    }
    onEnumMember(_enm, _member) {
        /* noop */
    }
    //
    // Fields
    // Can be used to implements properties backed by fields in cases where we want to generate "native" classes.
    // The default behavior is that properties do not have backing fields.
    hasField(_cls, _prop) {
        return false;
    }
    onField(_cls, _prop, _union) {
        /* noop */
    }
    visit(node, names = new Array()) {
        const namespace = !node.fqn && names.length > 0 ? names.join('.') : undefined;
        if (namespace) {
            this.onBeginNamespace(namespace);
        }
        const visitChildren = () => {
            Object.keys(node.children)
                .sort()
                .forEach((name) => {
                this.visit(node.children[name], names.concat(name));
            });
        };
        if (node.fqn) {
            const type = this.assembly.types?.[node.fqn];
            if (!type) {
                throw new Error(`Malformed jsii file. Cannot find type: ${node.fqn}`);
            }
            if (!this.shouldExcludeType(type.name)) {
                switch (type.kind) {
                    case spec.TypeKind.Class:
                        const classSpec = type;
                        const abstract = classSpec.abstract;
                        if (abstract && this.options.addBasePostfixToAbstractClassNames) {
                            this.addAbstractPostfixToClassName(classSpec);
                        }
                        this.onBeginClass(classSpec, abstract);
                        this.visitClass(classSpec);
                        visitChildren();
                        this.onEndClass(classSpec);
                        break;
                    case spec.TypeKind.Enum:
                        const enumSpec = type;
                        this.onBeginEnum(enumSpec);
                        this.visitEnum(enumSpec);
                        visitChildren();
                        this.onEndEnum(enumSpec);
                        break;
                    case spec.TypeKind.Interface:
                        const interfaceSpec = type;
                        this.onBeginInterface(interfaceSpec);
                        this.visitInterface(interfaceSpec);
                        visitChildren();
                        this.onEndInterface(interfaceSpec);
                        break;
                    default:
                        throw new Error(`Unsupported type kind: ${type.kind}`);
                }
            }
        }
        else {
            visitChildren();
        }
        if (namespace) {
            this.onEndNamespace(namespace);
        }
    }
    /**
     * Adds a postfix ("XxxBase") to the class name to indicate it is abstract.
     */
    addAbstractPostfixToClassName(cls) {
        cls.name = `${cls.name}Base`;
        const components = cls.fqn.split('.');
        cls.fqn = components
            .map((x, i) => (i < components.length - 1 ? x : `${x}Base`))
            .join('.');
    }
    excludeType(...names) {
        for (const n of names) {
            this.excludeTypes.push(n);
        }
    }
    shouldExcludeType(name) {
        return this.excludeTypes.includes(name);
    }
    /**
     * Returns all the method overloads needed to satisfy optional arguments.
     * For example, for the method `foo(bar: string, hello?: number, world?: number)`
     * this method will return:
     *  - foo(bar: string)
     *  - foo(bar: string, hello: number)
     *
     * Notice that the method that contains all the arguments will not be returned.
     */
    createOverloadsForOptionals(method) {
        const overloads = new Array();
        // if option disabled, just return the empty array.
        if (!this.options.generateOverloadsForMethodWithOptionals ||
            !method.parameters) {
            return overloads;
        }
        //
        // pop an argument from the end of the parameter list.
        // if it is an optional argument, clone the method without that parameter.
        // continue until we reach a non optional param or no parameters left.
        //
        const remaining = clone(method.parameters);
        let next;
        next = remaining.pop();
        // Parameter is optional if it's type is optional, and all subsequent parameters are optional/variadic
        while (next?.optional) {
            // clone the method but set the parameter list based on the remaining set of parameters
            const cloned = clone(method);
            cloned.parameters = clone(remaining);
            overloads.push(cloned);
            // pop the next parameter
            next = remaining.pop();
        }
        return overloads;
    }
    visitInterface(ifc) {
        if (ifc.properties) {
            ifc.properties.forEach((prop) => {
                this.onInterfaceProperty(ifc, prop);
            });
        }
        if (ifc.methods) {
            ifc.methods.forEach((method) => {
                this.onInterfaceMethod(ifc, method);
                for (const overload of this.createOverloadsForOptionals(method)) {
                    this.onInterfaceMethodOverload(ifc, overload, method);
                }
            });
        }
    }
    visitClass(cls) {
        const initializer = cls.initializer;
        if (initializer) {
            this.onInitializer(cls, initializer);
            // if method has optional arguments and
            for (const overload of this.createOverloadsForOptionals(initializer)) {
                this.onInitializerOverload(cls, overload, initializer);
            }
        }
        // if running in 'pure' mode and the class has methods, emit them as abstract methods.
        if (cls.methods) {
            this.onBeginMethods(cls);
            cls.methods.forEach((method) => {
                if (!method.static) {
                    this.onMethod(cls, method);
                    for (const overload of this.createOverloadsForOptionals(method)) {
                        this.onMethodOverload(cls, overload, method);
                    }
                }
                else {
                    this.onStaticMethod(cls, method);
                    for (const overload of this.createOverloadsForOptionals(method)) {
                        this.onStaticMethodOverload(cls, overload, method);
                    }
                }
            });
            this.onEndMethods(cls);
        }
        if (cls.properties) {
            this.onBeginProperties(cls);
            cls.properties.forEach((prop) => {
                if (this.hasField(cls, prop)) {
                    this.onField(cls, prop, spec.isUnionTypeReference(prop.type) ? prop.type : undefined);
                }
            });
            cls.properties.forEach((prop) => {
                if (!spec.isUnionTypeReference(prop.type)) {
                    if (!prop.static) {
                        this.onProperty(cls, prop);
                    }
                    else {
                        this.onStaticProperty(cls, prop);
                    }
                }
                else {
                    // okay, this is a union. some languages support unions (mostly the dynamic ones) and some will need some help
                    // if `expandUnionProperties` is set, we will "expand" each property that has a union type into multiple properties
                    // and postfix their name with the type name (i.e. FooAsToken).
                    // first, emit a property for the union, for languages that support unions.
                    this.onUnionProperty(cls, prop, prop.type);
                    // if require, we also "expand" the union for languages that don't support unions.
                    if (this.options.expandUnionProperties) {
                        for (const [index, type] of prop.type.union.types.entries()) {
                            // create a clone of this property
                            const propClone = clone(prop);
                            const primary = this.isPrimaryExpandedUnionProperty(prop.type, index);
                            const propertyName = primary
                                ? prop.name
                                : `${prop.name}As${this.displayNameForType(type)}`;
                            propClone.type = type;
                            propClone.optional = prop.optional;
                            propClone.name = propertyName;
                            this.onExpandedUnionProperty(cls, propClone, prop.name);
                        }
                    }
                }
            });
            this.onEndProperties(cls);
        }
    }
    /**
     * Magical heuristic to determine which type in a union is the primary type. The primary type will not have
     * a postfix with the name of the type attached to the expanded property name.
     *
     * The primary type is determined according to the following rules (first match):
     * 1. The first primitive type
     * 2. The first primitive collection
     * 3. No primary
     */
    isPrimaryExpandedUnionProperty(ref, index) {
        if (!ref) {
            return false;
        }
        return (index ===
            ref.union.types.findIndex((t) => {
                if (spec.isPrimitiveTypeReference(t)) {
                    return true;
                }
                return false;
            }));
    }
    visitEnum(enumSpec) {
        if (enumSpec.members) {
            enumSpec.members.forEach((spec) => this.onEnumMember(enumSpec, spec));
        }
    }
    displayNameForType(type) {
        // last name from FQN
        if (spec.isNamedTypeReference(type)) {
            const comps = type.fqn.split('.');
            const last = comps[comps.length - 1];
            return this.code.toPascalCase(last);
        }
        // primitive name
        if (spec.isPrimitiveTypeReference(type)) {
            return this.code.toPascalCase(type.primitive);
        }
        // ListOfX or MapOfX
        const coll = spec.isCollectionTypeReference(type) && type.collection;
        if (coll) {
            return `${this.code.toPascalCase(coll.kind)}Of${this.displayNameForType(coll.elementtype)}`;
        }
        const union = spec.isUnionTypeReference(type) && type.union;
        if (union) {
            return union.types.map((t) => this.displayNameForType(t)).join('Or');
        }
        throw new Error(`Cannot determine display name for type: ${JSON.stringify(type)}`);
    }
    /**
     * Looks up a jsii module in the dependency tree.
     * @param name The name of the jsii module to look up
     */
    findModule(name) {
        // if this is the current module, return it
        if (this.assembly.name === name) {
            return this.assembly;
        }
        const found = (this.assembly.dependencyClosure ?? {})[name];
        if (found) {
            return found;
        }
        throw new Error(`Unable to find module ${name} as a dependency of ${this.assembly.name}`);
    }
    findType(fqn) {
        const ret = this.reflectAssembly.system.tryFindFqn(fqn);
        if (!ret) {
            throw new Error(`Cannot find type '${fqn}' either as internal or external type`);
        }
        return ret.spec;
    }
}
exports.Generator = Generator;
//# sourceMappingURL=generator.js.map