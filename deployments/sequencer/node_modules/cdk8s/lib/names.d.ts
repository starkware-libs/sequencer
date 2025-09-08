import { Construct } from 'constructs';
/**
 * Options for name generation.
 */
export interface NameOptions {
    /**
     * Maximum allowed length for the name.
     * @default 63
     */
    readonly maxLen?: number;
    /**
     * Extra components to include in the name.
     * @default [] use the construct path components
     */
    readonly extra?: string[];
    /**
     * Delimiter to use between components.
     * @default "-"
     */
    readonly delimiter?: string;
    /**
     * Include a short hash as last part of the name.
     * @default true
     */
    readonly includeHash?: boolean;
}
/**
 * Utilities for generating unique and stable names.
 */
export declare class Names {
    /**
     * Generates a unique and stable name compatible DNS_LABEL from RFC-1123 from
     * a path.
     *
     * The generated name will:
     *  - contain at most 63 characters
     *  - contain only lowercase alphanumeric characters or ‘-’
     *  - start with an alphanumeric character
     *  - end with an alphanumeric character
     *
     * The generated name will have the form:
     *  <comp0>-<comp1>-..-<compN>-<short-hash>
     *
     * Where <comp> are the path components (assuming they are is separated by
     * "/").
     *
     * Note that if the total length is longer than 63 characters, we will trim
     * the first components since the last components usually encode more meaning.
     *
     * @link https://tools.ietf.org/html/rfc1123
     *
     * @param scope The construct for which to render the DNS label
     * @param options Name options
     * @throws if any of the components do not adhere to naming constraints or
     * length.
     */
    static toDnsLabel(scope: Construct, options?: NameOptions): string;
    /**
     * Generates a unique and stable name compatible label key name segment and
     * label value from a path.
     *
     * The name segment is required and must be 63 characters or less, beginning
     * and ending with an alphanumeric character ([a-z0-9A-Z]) with dashes (-),
     * underscores (_), dots (.), and alphanumerics between.
     *
     * Valid label values must be 63 characters or less and must be empty or
     * begin and end with an alphanumeric character ([a-z0-9A-Z]) with dashes
     * (-), underscores (_), dots (.), and alphanumerics between.
     *
     * The generated name will have the form:
     *  <comp0><delim><comp1><delim>..<delim><compN><delim><short-hash>
     *
     * Where <comp> are the path components (assuming they are is separated by
     * "/").
     *
     * Note that if the total length is longer than 63 characters, we will trim
     * the first components since the last components usually encode more meaning.
     *
     * @link https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#syntax-and-character-set
     *
     * @param scope The construct for which to render the DNS label
     * @param options Name options
     * @throws if any of the components do not adhere to naming constraints or
     * length.
     */
    static toLabelValue(scope: Construct, options?: NameOptions): string;
    private constructor();
}
