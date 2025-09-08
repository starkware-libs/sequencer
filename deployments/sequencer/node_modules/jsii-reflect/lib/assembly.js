"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Assembly = void 0;
const jsii = require("@jsii/spec");
const class_1 = require("./class");
const dependency_1 = require("./dependency");
const enum_1 = require("./enum");
const interface_1 = require("./interface");
const module_like_1 = require("./module-like");
const submodule_1 = require("./submodule");
class Assembly extends module_like_1.ModuleLike {
    constructor(system, spec) {
        super(system);
        this.spec = spec;
    }
    get fqn() {
        return this.spec.name;
    }
    /**
     * The version of the spec schema
     */
    get schema() {
        return this.spec.schema;
    }
    /**
     * The version of the jsii compiler that was used to produce this Assembly.
     */
    get jsiiVersion() {
        return this.spec.jsiiVersion;
    }
    /**
     * The name of the assembly
     */
    get name() {
        return this.spec.name;
    }
    /**
     * Description of the assembly, maps to "description" from package.json
     * This is required since some package managers (like Maven) require it.
     */
    get description() {
        return this.spec.description;
    }
    /**
     * The metadata associated with the assembly, if any.
     */
    get metadata() {
        return this.spec.metadata;
    }
    /**
     * The url to the project homepage. Maps to "homepage" from package.json.
     */
    get homepage() {
        return this.spec.homepage;
    }
    /**
     * The module repository, maps to "repository" from package.json
     * This is required since some package managers (like Maven) require it.
     */
    get repository() {
        return this.spec.repository;
    }
    /**
     * The main author of this package.
     */
    get author() {
        return this.spec.author;
    }
    /**
     * Additional contributors to this package.
     */
    get contributors() {
        return this.spec.contributors ?? [];
    }
    /**
     * A fingerprint that can be used to determine if the specification has changed.
     */
    get fingerprint() {
        return this.spec.fingerprint;
    }
    /**
     * The version of the assembly
     */
    get version() {
        return this.spec.version;
    }
    /**
     * The SPDX name of the license this assembly is distributed on.
     */
    get license() {
        return this.spec.license;
    }
    /**
     * A map of target name to configuration, which is used when generating packages for
     * various languages.
     */
    get targets() {
        return this.spec.targets;
    }
    /**
     * Dependencies on other assemblies (with semver), the key is the JSII assembly name.
     */
    get dependencies() {
        return Array.from(this._dependencies.values());
    }
    findDependency(name) {
        const dep = this._dependencies.get(name);
        if (!dep) {
            throw new Error(`Dependency ${name} not found for assembly ${this.name}`);
        }
        return dep;
    }
    /**
     * List if bundled dependencies (these are not expected to be jsii assemblies).
     */
    get bundled() {
        return this.spec.bundled ?? {};
    }
    /**
     * The top-level readme document for this assembly (if any).
     */
    get readme() {
        return this.spec.readme;
    }
    /**
     * Return the those submodules nested directly under the assembly
     */
    get submodules() {
        const { submodules } = this._analyzeTypes();
        return Array.from(submodules.entries())
            .filter(([name, _]) => name.split('.').length === 2)
            .map(([_, submodule]) => submodule);
    }
    /**
     * Return all submodules, even those transtively nested
     */
    get allSubmodules() {
        const { submodules } = this._analyzeTypes();
        return Array.from(submodules.values());
    }
    /**
     * All types in the assembly and all of its submodules
     */
    get allTypes() {
        return [...this.types, ...this.allSubmodules.flatMap((s) => s.types)];
    }
    /**
     * All classes in the assembly and all of its submodules
     */
    get allClasses() {
        return this.allTypes.filter((t) => t instanceof class_1.ClassType).map((t) => t);
    }
    /**
     * All interfaces in the assembly and all of its submodules
     */
    get allInterfaces() {
        return this.allTypes
            .filter((t) => t instanceof interface_1.InterfaceType)
            .map((t) => t);
    }
    /**
     * All interfaces in the assembly and all of its submodules
     */
    get allEnums() {
        return this.allTypes.filter((t) => t instanceof enum_1.EnumType).map((t) => t);
    }
    findType(fqn) {
        const type = this.tryFindType(fqn);
        if (!type) {
            throw new Error(`Type '${fqn}' not found in assembly ${this.name}`);
        }
        return type;
    }
    /**
     * Validate an assembly after loading
     *
     * If the assembly was loaded without validation, call this to validate
     * it after all. Throws an exception if validation fails.
     */
    validate() {
        jsii.validateAssembly(this.spec);
    }
    get submoduleMap() {
        return this._analyzeTypes().submodules;
    }
    /**
     * All types in the root of the assembly
     */
    get typeMap() {
        return this._analyzeTypes().types;
    }
    get _dependencies() {
        if (!this._dependencyCache) {
            this._dependencyCache = new Map();
            if (this.spec.dependencies) {
                for (const name of Object.keys(this.spec.dependencies)) {
                    this._dependencyCache.set(name, new dependency_1.Dependency(this.system, name, this.spec.dependencies[name]));
                }
            }
        }
        return this._dependencyCache;
    }
    _analyzeTypes() {
        if (!this._typeCache || !this._submoduleCache) {
            this._typeCache = new Map();
            const submoduleBuilders = this.discoverSubmodules();
            const ts = this.spec.types ?? {};
            for (const [fqn, typeSpec] of Object.entries(ts)) {
                let type;
                switch (typeSpec.kind) {
                    case jsii.TypeKind.Class:
                        type = new class_1.ClassType(this.system, this, typeSpec);
                        break;
                    case jsii.TypeKind.Interface:
                        type = new interface_1.InterfaceType(this.system, this, typeSpec);
                        break;
                    case jsii.TypeKind.Enum:
                        type = new enum_1.EnumType(this.system, this, typeSpec);
                        break;
                    default:
                        throw new Error('Unknown type kind');
                }
                // Find containing submodule (potentially through containing nested classes,
                // which DO count as namespaces but don't count as modules)
                let submodule = typeSpec.namespace;
                while (submodule != null && `${this.spec.name}.${submodule}` in ts) {
                    submodule = ts[`${this.spec.name}.${submodule}`].namespace;
                }
                if (submodule != null) {
                    const moduleName = `${this.spec.name}.${submodule}`;
                    submoduleBuilders.get(moduleName).addType(type);
                }
                else {
                    this._typeCache.set(fqn, type);
                }
            }
            this._submoduleCache = mapValues(submoduleBuilders, (b) => b.build());
        }
        return { types: this._typeCache, submodules: this._submoduleCache };
    }
    /**
     * Return a builder for all submodules in this assembly (so that we can
     * add types into the objects).
     */
    discoverSubmodules() {
        const system = this.system;
        const ret = new Map();
        for (const [submoduleName, submoduleSpec] of Object.entries(this.spec.submodules ?? {})) {
            ret.set(submoduleName, new SubmoduleBuilder(system, submoduleSpec, submoduleName, ret));
        }
        return ret;
    }
}
exports.Assembly = Assembly;
/**
 * Mutable Submodule builder
 *
 * Allows adding Types before the submodule is frozen to a Submodule class.
 *
 * Takes a reference to the full map of submodule builders, so that come time
 * to translate
 */
class SubmoduleBuilder {
    constructor(system, spec, fullName, allModuleBuilders) {
        this.system = system;
        this.spec = spec;
        this.fullName = fullName;
        this.allModuleBuilders = allModuleBuilders;
        this.types = new Map();
    }
    /**
     * Whether this submodule is a direct child of another submodule
     */
    isChildOf(other) {
        return (this.fullName.startsWith(`${other.fullName}.`) &&
            this.fullName.split('.').length === other.fullName.split('.').length + 1);
    }
    build() {
        this._built ?? (this._built = new submodule_1.Submodule(this.system, this.spec, this.fullName, mapValues(this.findSubmoduleBuilders(), (b) => b.build()), this.types));
        return this._built;
    }
    /**
     * Return all the builders from the map that are nested underneath ourselves.
     */
    findSubmoduleBuilders() {
        const ret = new Map();
        for (const [k, child] of this.allModuleBuilders) {
            if (child.isChildOf(this)) {
                ret.set(k, child);
            }
        }
        return ret;
    }
    addType(type) {
        this.types.set(type.fqn, type);
    }
}
function mapValues(xs, fn) {
    const ret = new Map();
    for (const [k, v] of xs) {
        ret.set(k, fn(v));
    }
    return ret;
}
//# sourceMappingURL=assembly.js.map