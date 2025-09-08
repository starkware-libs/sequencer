"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const validator_1 = require("../validator");
// A rule set for the compilerOptions of the strict family.
// The rule set enforces strict, but allows the defining options that are implied by strict
const strictFamilyOptions = new validator_1.RuleSet();
strictFamilyOptions.shouldPass('strict', validator_1.Match.eq(true));
strictFamilyOptions.shouldPass('alwaysStrict', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('noImplicitAny', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('noImplicitThis', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('strictBindCallApply', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('strictFunctionTypes', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('strictNullChecks', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('strictPropertyInitialization', validator_1.Match.optional(validator_1.Match.eq(true)));
strictFamilyOptions.shouldPass('useUnknownInCatchVariables', validator_1.Match.optional(validator_1.Match.eq(true)));
exports.default = strictFamilyOptions;
//# sourceMappingURL=strict-family-options.js.map