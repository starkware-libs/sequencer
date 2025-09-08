"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.schema = void 0;
exports.validateAssembly = validateAssembly;
const ajv_1 = require("ajv");
// eslint-disable-next-line @typescript-eslint/no-require-imports, @typescript-eslint/no-var-requires
exports.schema = require('../schema/jsii-spec.schema.json');
function validateAssembly(obj) {
    const ajv = new ajv_1.default({
        allErrors: true,
    });
    const validate = ajv.compile(exports.schema);
    validate(obj);
    if (validate.errors) {
        let descr = '';
        if (typeof obj.name === 'string' && obj.name !== '') {
            descr =
                typeof obj.version === 'string'
                    ? ` ${obj.name}@${obj.version}`
                    : ` ${obj.name}`;
        }
        throw new Error(`Invalid assembly${descr}:\n * ${ajv.errorsText(validate.errors, {
            separator: '\n * ',
            dataVar: 'assembly',
        })}`);
    }
    return obj;
}
//# sourceMappingURL=validate-assembly.js.map