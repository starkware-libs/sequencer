import { TypeScriptConfig, TypeScriptConfigValidationRuleSet } from '.';
export declare class TypeScriptConfigValidator {
    ruleSet: TypeScriptConfigValidationRuleSet;
    private readonly validator;
    private readonly compilerOptions;
    constructor(ruleSet: TypeScriptConfigValidationRuleSet);
    /**
     * Validated the provided config against the set of rules.
     *
     * @throws when the config is invalid
     *
     * @param tsconfig the tsconfig to be validated, this MUST be a tsconfig as a user would have written it in tsconfig.
     */
    validate(tsconfig: TypeScriptConfig): void;
}
//# sourceMappingURL=tsconfig-validator.d.ts.map