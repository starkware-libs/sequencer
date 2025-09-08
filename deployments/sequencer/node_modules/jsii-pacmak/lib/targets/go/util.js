"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.findTypeInTree = findTypeInTree;
exports.goPackageNameForAssembly = goPackageNameForAssembly;
exports.flatMap = flatMap;
exports.getMemberDependencies = getMemberDependencies;
exports.getParamDependencies = getParamDependencies;
exports.substituteReservedWords = substituteReservedWords;
exports.tarballName = tarballName;
/*
 * Recursively search module for type with fqn
 */
function findTypeInTree(module, fqn) {
    const result = module.types.find((t) => t.type.fqn === fqn);
    if (result) {
        return result;
    }
    return module.submodules.reduce((accum, sm) => {
        return accum ?? findTypeInTree(sm, fqn);
    }, undefined);
}
/*
 * Format NPM package names as idiomatic Go module name
 */
function goPackageNameForAssembly(assembly) {
    const config = assembly.targets?.go ?? {};
    if (config.packageName) {
        return config.packageName;
    }
    return assembly.name.replace(/[^a-z0-9.]/gi, '').toLowerCase();
}
function flatMap(collection, mapper) {
    return collection
        .map(mapper)
        .reduce((acc, elt) => acc.concat(elt), new Array());
}
/*
 * Return module dependencies of a class or interface members
 */
function getMemberDependencies(members) {
    return members.flatMap((member) => member.reference?.dependencies ?? []);
}
function getParamDependencies(methods) {
    return methods.flatMap(({ parameters }) => parameters.flatMap((param) => param.reference?.dependencies ?? []));
}
const RESERVED_WORDS = {
    break: 'break_',
    default: 'default_',
    func: 'func_',
    interface: 'interface_',
    select: 'select_',
    case: 'case_',
    defer: 'defer_',
    go: 'go_',
    map: 'map_',
    struct: 'struct_',
    chan: 'chan_',
    else: 'else_',
    goto: 'goto_',
    package: 'package_',
    switch: 'switch_',
    const: 'const_',
    fallthrough: 'fallthrough_',
    if: 'if_',
    range: 'range_',
    type: 'type_',
    continue: 'continue_',
    for: 'for_',
    import: 'import_',
    return: 'return_',
    var: 'var_',
    _: '_arg',
};
/*
 * Sanitize reserved words
 */
function substituteReservedWords(name) {
    return RESERVED_WORDS[name] || name;
}
/**
 * Computes a safe tarball name for the provided assembly.
 *
 * @param assm the assembly.
 *
 * @returns a tarball name.
 */
function tarballName(assm) {
    const name = assm.name.replace(/^@/, '').replace(/\//g, '-');
    return `${name}-${assm.version}.tgz`;
}
//# sourceMappingURL=util.js.map