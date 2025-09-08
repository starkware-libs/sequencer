"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const jsii_configured_options_1 = require("./jsii-configured-options");
const compiler_options_1 = require("../compiler-options");
const validator_1 = require("../validator");
// The public rule set used for the "generated" tsconfig validation setting.\
// The goal of this rule set is to ensure a tsconfig is compatible to the one jsii would generate for the user.
// It is explicitly enforcing option values that are used for the generated tsconfig,
// as well as options that can be configured via jsii settings. All other options are disallowed.
const generated = new validator_1.RuleSet({
    unexpectedFields: validator_1.RuleType.FAIL,
});
// import all options that are configurable via jsii settings
generated.import(jsii_configured_options_1.default);
// ... and all generated options
for (const [field, value] of Object.entries((0, compiler_options_1.convertForJson)(compiler_options_1.BASE_COMPILER_OPTIONS))) {
    if (typeof value === 'string') {
        generated.shouldPass(field, validator_1.Match.strEq(value, true));
        continue;
    }
    if (Array.isArray(value)) {
        generated.shouldPass(field, validator_1.Match.arrEq(value));
        continue;
    }
    generated.shouldPass(field, validator_1.Match.eq(value));
}
exports.default = generated;
//# sourceMappingURL=generated.public.js.map