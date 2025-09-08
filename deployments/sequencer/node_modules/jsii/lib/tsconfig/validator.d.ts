/**
 * A function that receives 3 arguments and validates if the provided value matches.
 * @param value The value to validate
 * @params options Additional options to influence the matcher behavior.
 * @returns true if the value matches
 */
type Matcher = (value: any, options?: MatcherOptions) => boolean;
interface MatcherOptions {
    /**
     * A function that will be called by the matcher with a a violation message.
     * This function is always called, regardless of the outcome of the matcher.
     * It is up to the caller of the matcher to decide if the message should be used or not.
     *
     * @param message The message describing the possible failure.
     */
    reporter?: (message: string) => void;
    /**
     * A function that might receive explicitly allowed values.
     * This can be used to generate synthetics values that would match the matcher.
     * It is not guaranteed that hints are received or that hints are complete.
     *
     * @param allowed The list values that a matcher offers as definitely allowed.
     */
    hints?: (allowed: any[]) => void;
}
export declare enum RuleType {
    PASS = 0,
    FAIL = 1
}
export interface RuleSetOptions {
    /**
     * Defines the behavior for any encountered fields for which no rules are defined.
     * The default is to pass these fields without validation,
     * but this can also be set to fail any unexpected fields.
     *
     * @default RuleType.PASS
     */
    readonly unexpectedFields: RuleType;
}
interface Rule {
    field: string;
    type: RuleType;
    matcher: Matcher;
}
export declare class RuleSet {
    readonly options: RuleSetOptions;
    private _rules;
    get rules(): Array<Rule>;
    /**
     * Return all fields for which a rule exists
     */
    get fields(): Array<string>;
    /**
     * Return a list of fields that are allowed, or undefined if all are allowed.
     */
    get allowedFields(): Array<string> | undefined;
    /**
     * Find all required fields by evaluating every rule in th set against undefined.
     * If the rule fails, the key must be required.
     *
     * @returns A list of keys that must be included or undefined
     */
    get requiredFields(): Array<string>;
    constructor(options?: RuleSetOptions);
    /**
     * Requires the matcher to pass for the given field.
     * Otherwise a violation is detected.
     *
     * @param field The field the rule applies to
     * @param matcher The matcher function
     */
    shouldPass(field: string, matcher: Matcher): void;
    /**
     * Detects a violation if the matcher is matching for a certain field.
     *
     * @param field The field the rule applies to
     * @param matcher The matcher function
     */
    shouldFail(field: string, matcher: Matcher): void;
    /**
     * Imports all rules from an other rule set.
     * Note that any options from the other rule set will be ignored.
     *
     * @param other The other rule set to import rules from.
     */
    import(other: RuleSet): void;
    /**
     * Records the field hints for the given rule set.
     * Hints are values that are guaranteed to pass the rule.
     * The list of hints is not guaranteed to be complete nor does it guarantee to return any values.
     * This can be used to create synthetic values for testing for error messages.
     *
     * @returns A record of fields and allowed values
     */
    getFieldHints(): Record<string, any[]>;
}
export declare class Match {
    /**
     * Value is optional, but if present should match
     */
    static optional(matcher: Matcher): Matcher;
    /**
     * Value must be one of the allowed options
     */
    static oneOf(...allowed: Array<string | number>): Matcher;
    /**
     * Value must be loosely equal to the expected value
     * Arrays are compared by elements
     */
    static eq(expected: any): Matcher;
    /**
     * Value must be loosely equal to the expected value
     * Arrays are compared by elements
     */
    static arrEq(expected: any[]): Matcher;
    /**
     * Compare strings, allows setting cases sensitivity
     */
    static strEq(expected: string, caseSensitive?: boolean): Matcher;
    /**
     * Allows any value
     */
    static ANY: Matcher;
    static TRUE: Matcher;
    static FALSE: Matcher;
    /**
     * Missing (undefined) value
     */
    static MISSING: Matcher;
}
export interface Violation {
    field: string;
    message: string;
}
export declare class ValidationError extends Error {
    readonly violations: Violation[];
    constructor(violations: Violation[]);
}
export declare class ObjectValidator {
    ruleSet: RuleSet;
    private readonly dataName;
    constructor(ruleSet: RuleSet, dataName?: string);
    /**
     * Validated the provided data against the set of rules.
     *
     * @throws when the data is invalid
     *
     * @param data the data to be validated
     */
    validate(data: {
        [field: string]: any;
    }): void;
}
export {};
//# sourceMappingURL=validator.d.ts.map