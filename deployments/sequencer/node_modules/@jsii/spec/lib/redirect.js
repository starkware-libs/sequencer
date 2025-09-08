"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.assemblyRedirectSchema = void 0;
exports.isAssemblyRedirect = isAssemblyRedirect;
exports.validateAssemblyRedirect = validateAssemblyRedirect;
const ajv_1 = require("ajv");
// eslint-disable-next-line @typescript-eslint/no-require-imports, @typescript-eslint/no-var-requires
exports.assemblyRedirectSchema = require('../schema/assembly-redirect.schema.json');
const SCHEMA = 'jsii/file-redirect';
/**
 * Checks whether the provided value is an assembly redirect. This only checks
 * for presence of the correct value in the `schema` attribute. For full
 * validation, `validateAssemblyRedirect` should be used instead.
 *
 * @param obj the value to be tested.
 *
 * @returns `true` if the value is indeed an AssemblyRedirect.
 */
function isAssemblyRedirect(obj) {
    if (typeof obj !== 'object' || obj == null) {
        return false;
    }
    return obj.schema === SCHEMA;
}
/**
 * Validates the provided value as an assembly redirect.
 *
 * @param obj the value to be tested.
 *
 * @returns the validated value.
 */
function validateAssemblyRedirect(obj) {
    const ajv = new ajv_1.default({
        allErrors: true,
    });
    const validate = ajv.compile(exports.assemblyRedirectSchema);
    validate(obj);
    if (validate.errors) {
        throw new Error(`Invalid assembly redirect:\n * ${ajv.errorsText(validate.errors, {
            separator: '\n * ',
            dataVar: 'redirect',
        })}`);
    }
    return obj;
}
//# sourceMappingURL=redirect.js.map