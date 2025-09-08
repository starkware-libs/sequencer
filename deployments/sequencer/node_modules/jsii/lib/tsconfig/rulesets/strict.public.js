"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const configurable_options_1 = require("./configurable-options");
const deprecated_options_1 = require("./deprecated-options");
const incompatible_options_1 = require("./incompatible-options");
const strict_family_options_1 = require("./strict-family-options");
const validator_1 = require("../validator");
// The public rule set used for the "strict" tsconfig validation setting.
// The goal of this rule set is to ensure a tsconfig that is following best practices for jsii.
// In practice, this is a combination of known incompatible options, known configurable options and additional best practices.
// The rule set also explicitly disallows unknown options.
const strict = new validator_1.RuleSet({
    unexpectedFields: validator_1.RuleType.FAIL,
});
// import all options that are configurable
strict.import(configurable_options_1.default);
// import all options that are definitely incompatible
strict.import(incompatible_options_1.default);
// strict family options
strict.import(strict_family_options_1.default);
// Best practice rules
strict.shouldPass('target', validator_1.Match.eq('es2022')); // node18
strict.shouldPass('lib', validator_1.Match.arrEq(['es2022'])); // node18
strict.shouldPass('module', validator_1.Match.oneOf('node16', 'commonjs'));
strict.shouldPass('moduleResolution', validator_1.Match.optional(validator_1.Match.oneOf('node', 'node16')));
strict.shouldPass('esModuleInterop', validator_1.Match.TRUE);
strict.shouldPass('skipLibCheck', validator_1.Match.TRUE);
strict.shouldPass('stripInternal', validator_1.Match.optional(validator_1.Match.FALSE));
strict.shouldPass('noEmitOnError', validator_1.Match.TRUE);
strict.shouldPass('declaration', validator_1.Match.TRUE);
// Deprecated ts options that should not be used with jsii
strict.import(deprecated_options_1.default);
exports.default = strict;
//# sourceMappingURL=strict.public.js.map