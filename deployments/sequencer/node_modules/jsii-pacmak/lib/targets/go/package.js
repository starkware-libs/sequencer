"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.InternalPackage = exports.RootPackage = exports.Package = exports.GO_VERSION = exports.GOMOD_FILENAME = void 0;
const path_1 = require("path");
const semver = require("semver");
const dependencies_1 = require("./dependencies");
const readme_file_1 = require("./readme-file");
const runtime_1 = require("./runtime");
const types_1 = require("./types");
const util_1 = require("./util");
const version_file_1 = require("./version-file");
const version_1 = require("../../version");
exports.GOMOD_FILENAME = 'go.mod';
exports.GO_VERSION = '1.23';
const MAIN_FILE = 'main.go';
/*
 * Package represents a single `.go` source file within a package. This can be the root package file or a submodule
 */
class Package {
    constructor(jsiiModule, packageName, filePath, moduleName, version, 
    // If no root is provided, this module is the root
    root) {
        this.jsiiModule = jsiiModule;
        this.packageName = packageName;
        this.filePath = filePath;
        this.moduleName = moduleName;
        this.version = version;
        this.embeddedTypes = new Map();
        this.directory = filePath;
        this.file = (0, path_1.join)(this.directory, `${packageName}.go`);
        this.root = root ?? this;
        this.submodules = this.jsiiModule.submodules.map((sm) => new InternalPackage(this.root, this, sm));
        this.types = this.jsiiModule.types.map((type) => {
            if (type.isInterfaceType() && type.datatype) {
                return new types_1.Struct(this, type);
            }
            else if (type.isInterfaceType()) {
                return new types_1.GoInterface(this, type);
            }
            else if (type.isClassType()) {
                return new types_1.GoClass(this, type);
            }
            else if (type.isEnumType()) {
                return new types_1.Enum(this, type);
            }
            throw new Error(`Type: ${type.name} with kind ${type.kind} is not a supported type`);
        });
        if (this.jsiiModule.readme?.markdown) {
            this.readmeFile = new readme_file_1.ReadmeFile(this.jsiiModule.fqn, this.jsiiModule.readme.markdown, this.directory);
        }
    }
    /*
     * Packages within this module
     */
    get dependencies() {
        return (0, util_1.flatMap)(this.types, (t) => t.dependencies).filter((mod) => mod.packageName !== this.packageName);
    }
    /*
     * goModuleName returns the full path to the module name.
     * Used for import statements and go.mod generation
     */
    get goModuleName() {
        const moduleName = this.root.moduleName;
        const prefix = moduleName !== '' ? `${moduleName}/` : '';
        const rootPackageName = this.root.packageName;
        const versionSuffix = determineMajorVersionSuffix(this.version);
        const suffix = this.filePath !== '' ? `/${this.filePath}` : ``;
        return `${prefix}${rootPackageName}${versionSuffix}${suffix}`;
    }
    /*
     * Search for a type with a `fqn` within this. Searches all Children modules as well.
     */
    findType(fqn) {
        return (0, util_1.findTypeInTree)(this, fqn);
    }
    emit(context) {
        this.emitTypes(context);
        this.readmeFile?.emit(context);
        this.emitGoInitFunction(context);
        this.emitSubmodules(context);
        this.emitInternal(context);
    }
    emitSubmodules(context) {
        for (const submodule of this.submodules) {
            submodule.emit(context);
        }
    }
    /**
     * Determines if `type` comes from a foreign package.
     */
    isExternalType(type) {
        return type.pkg !== this;
    }
    /**
     * Returns the name of the embed field used to embed a base class/interface in a
     * struct.
     *
     * @returns If the base is in the same package, returns the proxy name of the
     * base under `embed`, otherwise returns a unique symbol under `embed` and the
     * original interface reference under `original`.
     *
     * @param type The base type we want to embed
     */
    resolveEmbeddedType(type) {
        if (!this.isExternalType(type)) {
            return {
                embed: type.proxyName,
                fieldName: type.proxyName,
            };
        }
        const exists = this.embeddedTypes.get(type.fqn);
        if (exists) {
            return exists;
        }
        const typeref = new types_1.GoTypeRef(this.root, type.type.reference);
        const original = typeref.scopedName(this);
        const slug = original.replace(/[^A-Za-z0-9]/g, '');
        const aliasName = `Type__${slug}`;
        const embeddedType = {
            foriegnTypeName: original,
            foriegnType: typeref,
            fieldName: aliasName,
            embed: `${dependencies_1.INTERNAL_PACKAGE_NAME}.${aliasName}`,
        };
        this.embeddedTypes.set(type.fqn, embeddedType);
        return embeddedType;
    }
    emitHeader(code) {
        code.line(`package ${this.packageName}`);
        code.line();
    }
    /**
     * Emits a `func init() { ... }` in a dedicated file (so we don't have to
     * worry about what needs to be imported and whatnot). This function is
     * responsible for correctly initializing the module, including registering
     * the declared types with the jsii runtime for go.
     */
    emitGoInitFunction(context) {
        // We don't emit anything if there are not types in this (sub)module. This
        // avoids registering an `init` function that does nothing, which is poor
        // form. It also saves us from "imported but unused" errors that would arise
        // as a consequence.
        if (this.types.length > 0) {
            const { code } = context;
            const initFile = (0, path_1.join)(this.directory, MAIN_FILE);
            code.openFile(initFile);
            this.emitHeader(code);
            importGoModules(code, [dependencies_1.GO_REFLECT, dependencies_1.JSII_RT_MODULE]);
            code.line();
            code.openBlock('func init()');
            for (const type of this.types) {
                type.emitRegistration(context);
            }
            code.closeBlock();
            code.closeFile(initFile);
        }
    }
    emitImports(code, type) {
        const toImport = new Array();
        toImport.push(...(0, dependencies_1.toImportedModules)(type.specialDependencies, this));
        for (const goModuleName of new Set(type.dependencies.map(({ goModuleName }) => goModuleName))) {
            // If the module is the same as the current one being written, don't emit an import statement
            if (goModuleName !== this.goModuleName) {
                toImport.push({ module: goModuleName });
            }
        }
        importGoModules(code, toImport);
        code.line();
    }
    emitTypes(context) {
        for (const type of this.types) {
            const filePath = (0, path_1.join)(this.directory, `${type.name}.go`);
            context.code.openFile(filePath);
            this.emitHeader(context.code);
            this.emitImports(context.code, type);
            type.emit(context);
            context.code.closeFile(filePath);
            this.emitValidators(context, type);
        }
    }
    emitValidators({ code, runtimeTypeChecking }, type) {
        if (!runtimeTypeChecking) {
            return;
        }
        if (type.parameterValidators.length === 0 && type.structValidator == null) {
            return;
        }
        emit.call(this, (0, path_1.join)(this.directory, `${type.name}__checks.go`), false);
        emit.call(this, (0, path_1.join)(this.directory, `${type.name}__no_checks.go`), true);
        function emit(filePath, forNoOp) {
            code.openFile(filePath);
            // Conditional compilation tag...
            code.line(`//go:build ${forNoOp ? '' : '!'}no_runtime_type_checking`);
            code.line();
            this.emitHeader(code);
            if (!forNoOp) {
                const specialDependencies = (0, dependencies_1.reduceSpecialDependencies)(...type.parameterValidators.map((v) => v.specialDependencies), ...(type.structValidator
                    ? [type.structValidator.specialDependencies]
                    : []));
                importGoModules(code, [
                    ...(0, dependencies_1.toImportedModules)(specialDependencies, this),
                    ...Array.from(new Set([
                        ...(type.structValidator?.dependencies ?? []),
                        ...type.parameterValidators.flatMap((v) => v.dependencies),
                    ].map((mod) => mod.goModuleName)))
                        .filter((mod) => mod !== this.goModuleName)
                        .map((mod) => ({ module: mod })),
                ]);
                code.line();
            }
            else {
                code.line('// Building without runtime type checking enabled, so all the below just return nil');
                code.line();
            }
            type.structValidator?.emitImplementation(code, this, forNoOp);
            for (const validator of type.parameterValidators) {
                validator.emitImplementation(code, this, forNoOp);
            }
            code.closeFile(filePath);
        }
    }
    emitInternal(context) {
        if (this.embeddedTypes.size === 0) {
            return;
        }
        const code = context.code;
        const fileName = (0, path_1.join)(this.directory, dependencies_1.INTERNAL_PACKAGE_NAME, 'types.go');
        code.openFile(fileName);
        code.line(`package ${dependencies_1.INTERNAL_PACKAGE_NAME}`);
        const imports = new Set();
        for (const alias of this.embeddedTypes.values()) {
            if (!alias.foriegnType) {
                continue;
            }
            for (const pkg of alias.foriegnType.dependencies) {
                imports.add(pkg.goModuleName);
            }
        }
        code.open('import (');
        for (const imprt of imports) {
            code.line(`"${imprt}"`);
        }
        code.close(')');
        for (const alias of this.embeddedTypes.values()) {
            code.line(`type ${alias.fieldName} = ${alias.foriegnTypeName}`);
        }
        code.closeFile(fileName);
    }
}
exports.Package = Package;
/*
 * RootPackage corresponds to JSII module.
 *
 * Extends `Package` for root source package emit logic
 */
class RootPackage extends Package {
    constructor(assembly, rootPackageCache = new Map()) {
        const goConfig = assembly.targets?.go ?? {};
        const packageName = (0, util_1.goPackageNameForAssembly)(assembly);
        const filePath = '';
        const moduleName = goConfig.moduleName ?? '';
        const version = `${assembly.version}${goConfig.versionSuffix ?? ''}`;
        super(assembly, packageName, filePath, moduleName, version);
        this.typeCache = new Map();
        this.rootPackageCache = rootPackageCache;
        this.rootPackageCache.set(assembly.name, this);
        this.assembly = assembly;
        this.version = version;
        this.versionFile = new version_file_1.VersionFile(this.version);
    }
    emit(context) {
        super.emit(context);
        this.emitJsiiPackage(context);
        this.emitGomod(context.code);
        this.versionFile.emit(context.code);
    }
    emitGomod(code) {
        code.openFile(exports.GOMOD_FILENAME);
        code.line(`module ${this.goModuleName}`);
        code.line();
        code.line(`go ${exports.GO_VERSION}`);
        code.line();
        code.open('require (');
        // Strip " (build abcdef)" from the jsii version
        code.line(`${runtime_1.JSII_RT_MODULE_NAME} v${version_1.VERSION}`);
        const dependencies = this.packageDependencies;
        for (const dep of dependencies) {
            code.line(`${dep.goModuleName} v${dep.version}`);
        }
        indirectDependencies(dependencies, new Set(dependencies.map((dep) => dep.goModuleName)));
        code.close(')');
        code.closeFile(exports.GOMOD_FILENAME);
        /**
         * Emits indirect dependency declarations, which are helpful to make IDEs at
         * ease with the codebase.
         */
        function indirectDependencies(pkgs, alreadyEmitted) {
            for (const pkg of pkgs) {
                const deps = pkg.packageDependencies;
                for (const dep of deps) {
                    if (alreadyEmitted.has(dep.goModuleName)) {
                        continue;
                    }
                    alreadyEmitted.add(dep.goModuleName);
                    code.line(`${dep.goModuleName} v${dep.version} // indirect`);
                }
                indirectDependencies(deps, alreadyEmitted);
            }
        }
    }
    /*
     * Override package findType for root Package.
     *
     * This allows resolving type references from other JSII modules
     */
    findType(fqn) {
        if (!this.typeCache.has(fqn)) {
            this.typeCache.set(fqn, this.packageDependencies.reduce((accum, current) => {
                if (accum) {
                    return accum;
                }
                return current.findType(fqn);
            }, super.findType(fqn)));
        }
        return this.typeCache.get(fqn);
    }
    /*
     * Get all JSII module dependencies of the package being generated
     */
    get packageDependencies() {
        return this.assembly.dependencies.map((dep) => this.rootPackageCache.get(dep.assembly.name) ??
            new RootPackage(dep.assembly, this.rootPackageCache));
    }
    emitHeader(code) {
        const currentFilePath = code.getCurrentFilePath();
        if (this.assembly.description !== '' &&
            currentFilePath !== undefined &&
            currentFilePath.includes(MAIN_FILE)) {
            code.line(`// ${this.assembly.description}`);
        }
        code.line(`package ${this.packageName}`);
        code.line();
    }
    emitJsiiPackage({ code }) {
        const dependencies = this.packageDependencies.sort((l, r) => l.moduleName.localeCompare(r.moduleName));
        const file = (0, path_1.join)(runtime_1.JSII_INIT_PACKAGE, `${runtime_1.JSII_INIT_PACKAGE}.go`);
        code.openFile(file);
        code.line(`// Package ${runtime_1.JSII_INIT_PACKAGE} contains the functionaility needed for jsii packages to`);
        code.line('// initialize their dependencies and themselves. Users should never need to use this package');
        code.line('// directly. If you find you need to - please report a bug at');
        code.line('// https://github.com/aws/jsii/issues/new/choose');
        code.line(`package ${runtime_1.JSII_INIT_PACKAGE}`);
        code.line();
        const toImport = [
            dependencies_1.JSII_RT_MODULE,
            { module: 'embed', alias: '_' },
        ];
        if (dependencies.length > 0) {
            for (const pkg of dependencies) {
                toImport.push({
                    alias: pkg.packageName,
                    module: `${pkg.root.goModuleName}/${runtime_1.JSII_INIT_PACKAGE}`,
                });
            }
        }
        importGoModules(code, toImport);
        code.line();
        code.line(`//go:embed ${(0, util_1.tarballName)(this.assembly)}`);
        code.line('var tarball []byte');
        code.line();
        code.line(`// ${runtime_1.JSII_INIT_FUNC} loads the necessary packages in the @jsii/kernel to support the enclosing module.`);
        code.line('// The implementation is idempotent (and hence safe to be called over and over).');
        code.open(`func ${runtime_1.JSII_INIT_FUNC}() {`);
        if (dependencies.length > 0) {
            code.line('// Ensure all dependencies are initialized');
            for (const pkg of this.packageDependencies) {
                code.line(`${pkg.packageName}.${runtime_1.JSII_INIT_FUNC}()`);
            }
            code.line();
        }
        code.line('// Load this library into the kernel');
        code.line(`${runtime_1.JSII_RT_ALIAS}.Load("${this.assembly.name}", "${this.assembly.version}", tarball)`);
        code.close('}');
        code.closeFile(file);
    }
}
exports.RootPackage = RootPackage;
/*
 * InternalPackage refers to any go package within a given JSII module.
 */
class InternalPackage extends Package {
    constructor(root, parent, assembly) {
        const packageName = (0, util_1.goPackageNameForAssembly)(assembly);
        const filePath = parent === root ? packageName : `${parent.filePath}/${packageName}`;
        super(assembly, packageName, filePath, root.moduleName, root.version, root);
        this.parent = parent;
    }
}
exports.InternalPackage = InternalPackage;
/**
 * Go requires that when a module major version is v2.0 and above, the module
 * name will have a `/vNN` suffix (where `NN` is the major version).
 *
 * > Starting with major version 2, module paths must have a major version
 * > suffix like /v2 that matches the major version. For example, if a module
 * > has the path example.com/mod at v1.0.0, it must have the path
 * > example.com/mod/v2 at version v2.0.0.
 *
 * @see https://golang.org/ref/mod#major-version-suffixes
 * @param version The module version (e.g. `2.3.0`)
 * @returns a suffix to append to the module name in the form (`/vNN`). If the
 * module version is `0.x` or `1.x`, returns an empty string.
 */
function determineMajorVersionSuffix(version) {
    const sv = semver.parse(version);
    if (!sv) {
        throw new Error(`Unable to parse version "${version}" as a semantic version`);
    }
    // suffix is only needed for 2.0 and above
    if (sv.major <= 1) {
        return '';
    }
    return `/v${sv.major}`;
}
function importGoModules(code, modules) {
    if (modules.length === 0) {
        return;
    }
    const aliasSize = Math.max(...modules.map((mod) => mod.alias?.length ?? 0));
    code.open('import (');
    const sortedModules = Array.from(modules).sort(compareImportedModules);
    for (let i = 0; i < sortedModules.length; i++) {
        const mod = sortedModules[i];
        // Separate module categories from each other modules with a blank line.
        if (i > 0 &&
            (isBuiltIn(mod) !== isBuiltIn(sortedModules[i - 1]) ||
                isSpecial(mod) !== isSpecial(sortedModules[i - 1]))) {
            code.line();
        }
        if (mod.alias) {
            code.line(`${mod.alias.padEnd(aliasSize, ' ')} "${mod.module}"`);
        }
        else {
            code.line(`"${mod.module}"`);
        }
    }
    code.close(')');
    /**
     * A comparator for `ImportedModule` instances such that built-in modules
     * always appear first, followed by the rest. Then within these two groups,
     * aliased imports appear first, followed by the rest.
     */
    function compareImportedModules(l, r) {
        const lBuiltIn = isBuiltIn(l);
        const rBuiltIn = isBuiltIn(r);
        if (lBuiltIn && !rBuiltIn) {
            return -1;
        }
        if (!lBuiltIn && rBuiltIn) {
            return 1;
        }
        const lSpecial = isSpecial(l);
        const rSpecial = isSpecial(r);
        if (lSpecial && !rSpecial) {
            return -1;
        }
        if (!lSpecial && rSpecial) {
            return 1;
        }
        return l.module.localeCompare(r.module);
    }
    function isBuiltIn(mod) {
        // Standard library modules don't have any "." in their path, whereas any
        // other module has a DNS portion in them, which must include a ".".
        return !mod.module.includes('.');
    }
    function isSpecial(mod) {
        return mod.alias === runtime_1.JSII_RT_ALIAS || mod.alias === runtime_1.JSII_INIT_ALIAS;
    }
}
//# sourceMappingURL=package.js.map