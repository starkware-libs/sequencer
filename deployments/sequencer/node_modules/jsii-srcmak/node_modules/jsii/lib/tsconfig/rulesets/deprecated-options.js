"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const validator_1 = require("../validator");
// A rule set for deprecated compilerOptions that should not be used with jsii
// This is an internal rule set, that may be used by other rule sets.
const deprecatedOptions = new validator_1.RuleSet();
deprecatedOptions.shouldPass('prepend', validator_1.Match.MISSING);
exports.default = deprecatedOptions;
//# sourceMappingURL=deprecated-options.js.map