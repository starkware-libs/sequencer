"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.emitInitialization = emitInitialization;
exports.slugify = slugify;
const constants_1 = require("./constants");
// Emits call to initialize runtime client if not already
function emitInitialization(code) {
    code.line(`${constants_1.JSII_INIT_ALIAS}.${constants_1.JSII_INIT_FUNC}()`);
}
/**
 * Slugify a name by appending '_' at the end until the resulting name is not
 * present in the list of reserved names.
 *
 * @param name     the name to be slugified
 * @param reserved the list of names that are already sued in-scope
 *
 * @returns the slugified name
 */
function slugify(name, reserved) {
    const used = new Set(reserved);
    while (used.has(name)) {
        name += '_';
    }
    return name;
}
//# sourceMappingURL=util.js.map