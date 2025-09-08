"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TypeSystem = void 0;
const spec_1 = require("@jsii/spec");
const fs = require("fs-extra");
const path = require("path");
const assembly_1 = require("./assembly");
const class_1 = require("./class");
const enum_1 = require("./enum");
const interface_1 = require("./interface");
const util_1 = require("./util");
class TypeSystem {
    constructor() {
        /**
         * The "root" assemblies (ones that loaded explicitly via a "load" call).
         */
        this.roots = new Array();
        this._assemblyLookup = new Map();
        this._cachedClasses = new Map();
        this._locked = false;
    }
    get isLocked() {
        return this._locked;
    }
    /**
     * All assemblies in this type system.
     */
    get assemblies() {
        return Array.from(this._assemblyLookup.values());
    }
    /**
     * Locks the TypeSystem from further changes
     *
     * Call this once all assemblies have been loaded.
     * This allows the reflection to optimize and cache certain expensive calls.
     */
    lock() {
        this._locked = true;
    }
    /**
     * Load all JSII dependencies of the given NPM package directory.
     *
     * The NPM package itself does *not* have to be a jsii package, and does
     * NOT have to declare a JSII dependency on any of the packages.
     */
    async loadNpmDependencies(packageRoot, options = {}) {
        const pkg = await fs.readJson(path.resolve(packageRoot, 'package.json'));
        for (const dep of dependenciesOf(pkg)) {
            if ((0, util_1.isBuiltinModule)(dep)) {
                continue;
            }
            // eslint-disable-next-line no-await-in-loop
            const depDir = await (0, util_1.findDependencyDirectory)(dep, packageRoot);
            // eslint-disable-next-line no-await-in-loop
            const depPkgJson = await fs.readJson(path.join(depDir, 'package.json'));
            if (!depPkgJson.jsii) {
                continue;
            }
            // eslint-disable-next-line no-await-in-loop
            await this.loadModule(depDir, options);
        }
    }
    /**
     * Loads a jsii module or a single .jsii file into the type system.
     *
     * If `fileOrDirectory` is a directory, it will be treated as a jsii npm module,
     * and its dependencies (as determined by its 'package.json' file) will be loaded
     * as well.
     *
     * If `fileOrDirectory` is a file, it will be treated as a single .jsii file.
     * No dependencies will be loaded. You almost never want this.
     *
     * Not validating makes the difference between loading assemblies with lots
     * of dependencies (such as app-delivery) in 90ms vs 3500ms.
     *
     * @param fileOrDirectory A .jsii file path or a module directory
     * @param validate Whether or not to validate the assembly while loading it.
     */
    async load(fileOrDirectory, options = {}) {
        if ((await fs.stat(fileOrDirectory)).isDirectory()) {
            return this.loadModule(fileOrDirectory, options);
        }
        return this.loadFile(fileOrDirectory, { ...options, isRoot: true });
    }
    async loadModule(dir, options = {}) {
        const out = await _loadModule.call(this, dir, true);
        if (!out) {
            throw new Error(`Unable to load module from directory: ${dir}`);
        }
        return out;
        async function _loadModule(moduleDirectory, isRoot = false) {
            const filePath = path.join(moduleDirectory, 'package.json');
            const pkg = JSON.parse(await fs.readFile(filePath, { encoding: 'utf-8' }));
            if (!pkg.jsii) {
                throw new Error(`No "jsii" section in ${filePath}`);
            }
            // Load the assembly, but don't recurse if we already have an assembly with the same name.
            // Validation is not an insignificant time sink, and loading IS insignificant, so do a
            // load without validation first. This saves about 2/3rds of processing time.
            const asm = this.loadAssembly((0, spec_1.findAssemblyFile)(moduleDirectory), false);
            if (this.includesAssembly(asm.name)) {
                const existing = this.findAssembly(asm.name);
                if (existing.version !== asm.version) {
                    throw new Error(`Conflicting versions of ${asm.name} in type system: previously loaded ${existing.version}, trying to load ${asm.version}`);
                }
                // Make sure that we mark this thing as root after all if it wasn't yet.
                if (isRoot) {
                    this.addRoot(asm);
                }
                return existing;
            }
            if (options.validate !== false) {
                asm.validate();
            }
            const root = this.addAssembly(asm, { isRoot });
            // Using || instead of ?? because npmjs.com will alter the package.json file and possibly put `false` in pkg.bundleDependencies.
            // This is actually non compliant to the package.json specification, but that's how it is...
            const bundled = pkg.bundledDependencies ?? pkg.bundleDependencies ?? [];
            for (const name of dependenciesOf(pkg)) {
                if (bundled.includes(name)) {
                    continue;
                }
                // eslint-disable-next-line no-await-in-loop
                const depDir = await (0, util_1.findDependencyDirectory)(name, moduleDirectory);
                // eslint-disable-next-line no-await-in-loop
                await _loadModule.call(this, depDir);
            }
            return root;
        }
    }
    loadFile(file, options = {}) {
        const assembly = this.loadAssembly(file, options.validate !== false);
        return this.addAssembly(assembly, options);
    }
    addAssembly(asm, options = {}) {
        if (this.isLocked) {
            throw new Error('The typesystem has been locked from further changes');
        }
        if (asm.system !== this) {
            throw new Error('Assembly has been created for different typesystem');
        }
        if (!this._assemblyLookup.has(asm.name)) {
            this._assemblyLookup.set(asm.name, asm);
        }
        if (options.isRoot !== false) {
            this.addRoot(asm);
        }
        return asm;
    }
    /**
     * Determines whether this TypeSystem includes a given assembly.
     *
     * @param name the name of the assembly being looked for.
     */
    includesAssembly(name) {
        return this._assemblyLookup.has(name);
    }
    isRoot(name) {
        return this.roots.map((r) => r.name).includes(name);
    }
    findAssembly(name) {
        const ret = this.tryFindAssembly(name);
        if (!ret) {
            throw new Error(`Assembly "${name}" not found`);
        }
        return ret;
    }
    tryFindAssembly(name) {
        return this._assemblyLookup.get(name);
    }
    findFqn(fqn) {
        const [assembly] = fqn.split('.');
        const asm = this.findAssembly(assembly);
        return asm.findType(fqn);
    }
    tryFindFqn(fqn) {
        const [assembly] = fqn.split('.');
        const asm = this.tryFindAssembly(assembly);
        return asm?.tryFindType(fqn);
    }
    findClass(fqn) {
        const type = this.findFqn(fqn);
        if (!(type instanceof class_1.ClassType)) {
            throw new Error(`FQN ${fqn} is not a class`);
        }
        return type;
    }
    findInterface(fqn) {
        const type = this.findFqn(fqn);
        if (!(type instanceof interface_1.InterfaceType)) {
            throw new Error(`FQN ${fqn} is not an interface`);
        }
        return type;
    }
    findEnum(fqn) {
        const type = this.findFqn(fqn);
        if (!(type instanceof enum_1.EnumType)) {
            throw new Error(`FQN ${fqn} is not an enum`);
        }
        return type;
    }
    /**
     * All methods in the type system.
     */
    get methods() {
        const getMethods = (mod) => {
            return [
                ...flatMap(mod.submodules, getMethods),
                ...flatMap(mod.interfaces, (iface) => iface.ownMethods),
                ...flatMap(mod.classes, (clazz) => clazz.ownMethods),
            ];
        };
        return flatMap(this.assemblies, getMethods);
    }
    /**
     * All properties in the type system.
     */
    get properties() {
        const getProperties = (mod) => {
            return [
                ...flatMap(mod.submodules, getProperties),
                ...flatMap(mod.interfaces, (iface) => iface.ownProperties),
                ...flatMap(mod.classes, (clazz) => clazz.ownProperties),
            ];
        };
        return flatMap(this.assemblies, getProperties);
    }
    /**
     * All classes in the type system.
     */
    get classes() {
        const out = new Array();
        this.assemblies.forEach((a) => {
            // Cache the class list for each assembly. We can't use @memoized for this method since new
            // assemblies can be added between calls, via loadModule().
            if (!this._cachedClasses.has(a)) {
                this._cachedClasses.set(a, collectTypes(a, (item) => item.classes));
            }
            out.push(...this._cachedClasses.get(a));
        });
        return out;
    }
    /**
     * All interfaces in the type system.
     */
    get interfaces() {
        const out = new Array();
        this.assemblies.forEach((a) => {
            out.push(...collectTypes(a, (item) => item.interfaces));
        });
        return out;
    }
    /**
     * All enums in the type system.
     */
    get enums() {
        const out = new Array();
        this.assemblies.forEach((a) => {
            out.push(...collectTypes(a, (item) => item.enums));
        });
        return out;
    }
    /**
     * Load an assembly without adding it to the typesystem
     * @param file Assembly file to load
     * @param validate Whether to validate the assembly or just assume it matches the schema
     */
    loadAssembly(file, validate = true) {
        const contents = (0, spec_1.loadAssemblyFromFile)(file, validate);
        return new assembly_1.Assembly(this, contents);
    }
    addRoot(asm) {
        if (!this.roots.some((r) => r.name === asm.name)) {
            this.roots.push(asm);
        }
    }
}
exports.TypeSystem = TypeSystem;
function dependenciesOf(packageJson) {
    const deps = new Set();
    Object.keys(packageJson.dependencies ?? {}).forEach(deps.add.bind(deps));
    Object.keys(packageJson.peerDependencies ?? {}).forEach(deps.add.bind(deps));
    return Array.from(deps);
}
function collectTypes(module, getter) {
    const result = new Array();
    for (const submodule of module.submodules) {
        result.push(...collectTypes(submodule, getter));
    }
    result.push(...getter(module));
    return result;
}
function flatMap(collection, mapper) {
    return collection
        .map(mapper)
        .reduce((acc, elt) => acc.concat(elt), new Array());
}
//# sourceMappingURL=type-system.js.map