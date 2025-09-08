"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.GO_REFLECT = exports.JSII_RT_MODULE = exports.INTERNAL_PACKAGE_NAME = void 0;
exports.reduceSpecialDependencies = reduceSpecialDependencies;
exports.toImportedModules = toImportedModules;
const assert = require("assert");
const runtime_1 = require("./runtime");
function reduceSpecialDependencies(...specialDepsList) {
    const [first, ...rest] = specialDepsList;
    if (!first) {
        assert(rest.length === 0);
        return {
            fmt: false,
            init: false,
            internal: false,
            runtime: false,
            time: false,
        };
    }
    return rest.reduce((acc, elt) => ({
        fmt: acc.fmt || elt.fmt,
        init: acc.init || elt.init,
        internal: acc.internal || elt.internal,
        runtime: acc.runtime || elt.runtime,
        time: acc.time || elt.time,
    }), first);
}
function toImportedModules(specialDeps, context) {
    const result = new Array();
    if (specialDeps.fmt) {
        result.push({ module: 'fmt' });
    }
    if (specialDeps.time) {
        result.push({ module: 'time' });
    }
    if (specialDeps.runtime) {
        result.push(exports.JSII_RT_MODULE);
    }
    if (specialDeps.init) {
        result.push({
            alias: runtime_1.JSII_INIT_ALIAS,
            module: `${context.root.goModuleName}/${runtime_1.JSII_INIT_PACKAGE}`,
        });
    }
    if (specialDeps.internal) {
        result.push({
            module: `${context.goModuleName}/${exports.INTERNAL_PACKAGE_NAME}`,
        });
    }
    return result;
}
/**
 * The name of a sub-package that includes internal type aliases it has to be
 * "internal" so it not published.
 */
exports.INTERNAL_PACKAGE_NAME = 'internal';
exports.JSII_RT_MODULE = {
    alias: runtime_1.JSII_RT_ALIAS,
    module: runtime_1.JSII_RT_PACKAGE_NAME,
};
exports.GO_REFLECT = { module: 'reflect' };
//# sourceMappingURL=dependencies.js.map