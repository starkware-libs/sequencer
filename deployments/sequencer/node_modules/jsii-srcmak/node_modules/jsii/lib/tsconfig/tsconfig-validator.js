"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TypeScriptConfigValidator = void 0;
const generated_public_1 = require("./rulesets/generated.public");
const minimal_public_1 = require("./rulesets/minimal.public");
const strict_public_1 = require("./rulesets/strict.public");
const validator_1 = require("./validator");
const RuleSets = {
    generated: generated_public_1.default,
    strict: strict_public_1.default,
    minimal: minimal_public_1.default,
    off: new validator_1.RuleSet(),
};
class TypeScriptConfigValidator {
    constructor(ruleSet) {
        this.ruleSet = ruleSet;
        const topLevelRules = new validator_1.RuleSet({
            unexpectedFields: validator_1.RuleType.PASS,
        });
        topLevelRules.shouldPass('files', validator_1.Match.ANY);
        topLevelRules.shouldPass('extends', validator_1.Match.ANY);
        topLevelRules.shouldPass('include', validator_1.Match.ANY);
        topLevelRules.shouldPass('exclude', validator_1.Match.ANY);
        topLevelRules.shouldPass('references', validator_1.Match.ANY);
        topLevelRules.shouldPass('watchOptions', validator_1.Match.ANY);
        topLevelRules.shouldPass('typeAcquisition', validator_1.Match.MISSING);
        this.compilerOptions = new validator_1.ObjectValidator(RuleSets[ruleSet], 'compilerOptions');
        topLevelRules.shouldPass('compilerOptions', (compilerOptions) => {
            this.compilerOptions.validate(compilerOptions);
            return true;
        });
        this.validator = new validator_1.ObjectValidator(topLevelRules, 'tsconfig');
    }
    /**
     * Validated the provided config against the set of rules.
     *
     * @throws when the config is invalid
     *
     * @param tsconfig the tsconfig to be validated, this MUST be a tsconfig as a user would have written it in tsconfig.
     */
    validate(tsconfig) {
        this.validator.validate(tsconfig);
    }
}
exports.TypeScriptConfigValidator = TypeScriptConfigValidator;
//# sourceMappingURL=tsconfig-validator.js.map