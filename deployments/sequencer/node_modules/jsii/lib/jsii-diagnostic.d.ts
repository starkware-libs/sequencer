import * as spec from '@jsii/spec';
import * as ts from 'typescript';
import { TypeSystemHints } from './docs';
import { Violation } from './tsconfig/validator';
/**
 * Descriptors for all valid jsii diagnostic codes.
 *
 * The `category` or non-error codes can be updated, for example to treat
 * warnings as errors, or to suppress certain undesirable warnings.
 */
export declare class Code<T extends DiagnosticMessageFormatter = DiagnosticMessageFormatter> {
    #private;
    readonly code: number;
    readonly name: string;
    /**
     * Get a diagnostic code by code or name.
     *
     * @param codeOrName the looked up diagnostic code or name.
     *
     * @returns the JsiiDiagnosticCode instande, if one exists, or `undefined`
     *
     * @experimental this module is under active development and the error codes
     *               and names may change in the future.
     */
    static lookup(codeOrName: string | number): Code | undefined;
    private static readonly byCode;
    private static readonly byName;
    /**
     * Registers a new diagnostic code.
     *
     * @param code            the numeric code for the diagnostic
     * @param name            the symbolic name for the diagnostic
     * @param defaultCategory the default category this diagnostic ranks in
     * @param formatter       a message formatter for easy creation of diagnostics
     */
    private constructor();
    /**
     * Determines whether this diagnostic is a compilation error. Diagnostics
     * where this is `true` cannot have their `category` overridden to a lower
     * category.
     */
    get isError(): boolean;
    /**
     * The diagnostic category this particular code is filed as.
     */
    get category(): ts.DiagnosticCategory;
    /**
     * Update the diagnostic category for this particular code. If `isError` is
     * `true`, attempting to set anything other than `ts.DiagnosticCategory.Error`
     * will result in an error being throw.
     *
     * @param newValue the new diagnostic category to be used.
     */
    set category(newValue: ts.DiagnosticCategory);
    /**
     * Creates a new `JsiiDiagnostic` message without any source code location
     * data.
     *
     * @param args the arguments to the message formatter.
     *
     * @deprecated It is preferred to specify a source code location for problem
     *             markers. Prefer the use of `create` while providing a value
     *             for the `location` parameter whenever possible.
     */
    createDetached(...args: Parameters<T>): JsiiDiagnostic;
    /**
     * Creates a new `JsiiDiagnostic` message with source code location denoted
     * by the provided `location` node.
     *
     * @param location the source code location attachment of the message.
     * @param args     the arguments to the message formatter.
     */
    create(location: ts.Node | undefined, ...args: Parameters<T>): JsiiDiagnostic;
}
/**
 * A jsii-specific diagnostic entry.
 */
export declare class JsiiDiagnostic implements ts.Diagnostic {
    #private;
    static readonly JSII_0001_PKG_MISSING_DESCRIPTION: Code<() => string>;
    static readonly JSII_0002_PKG_MISSING_HOMEPAGE: Code<() => string>;
    static readonly JSII_0003_MISSING_README: Code<() => string>;
    static readonly JSII_0004_COULD_NOT_FIND_ENTRYPOINT: Code<(mainFile: string) => string>;
    static readonly JSII_0005_MISSING_PEER_DEPENDENCY: Code<(assm: string, reference: string) => string>;
    static readonly JSII_0006_MISSING_DEV_DEPENDENCY: Code<(dependencyName: string, peerRange: string, minVersion: string, actual: string) => string>;
    static readonly JSII_0007_MISSING_WARNINGS_EXPORT: Code<() => string>;
    static readonly JSII_1000_NO_CONST_ENUM: Code<() => string>;
    static readonly JSII_1001_TYPE_HAS_NO_SYMBOL: Code<() => string>;
    static readonly JSII_1002_UNSPECIFIED_PROMISE: Code<() => string>;
    static readonly JSII_1003_UNSUPPORTED_TYPE: Code<(messageText: any) => any>;
    static readonly JSII_1004_DUPLICATE_ENUM_VALUE: Code<(enumValue: string, enumMemberNames: string[]) => string>;
    static readonly JSII_1005_SEPARATE_WRITE_TYPE: Code<() => string>;
    static readonly JSII_1006_GENERIC_TYPE: Code<() => string>;
    static readonly JSII_1999_UNSUPPORTED: Code<({ what, alternative, suggestInternal, }: {
        what: string;
        alternative?: string;
        suggestInternal?: boolean;
    }) => string>;
    static readonly JSII_2000_MISSING_DIRECTIVE_ARGUMENT: Code<() => string>;
    static readonly JSII_2100_STRUCT_ON_NON_INTERFACE: Code<() => string>;
    static readonly JSII_2999_UNKNOWN_DIRECTIVE: Code<(text: string) => string>;
    static readonly JSII_3000_EXPORTED_API_USES_HIDDEN_TYPE: Code<(badFqn: any) => string>;
    static readonly JSII_3001_EXPOSED_INTERNAL_TYPE: Code<(symbol: ts.Symbol, isThisType: boolean, typeUse: string) => string>;
    static readonly JSII_3002_USE_OF_UNEXPORTED_FOREIGN_TYPE: Code<(fqn: string, typeUse: string, pkg: {
        readonly name: string;
    }) => string>;
    static readonly JSII_3003_SYMBOL_IS_EXPORTED_TWICE: Code<(ns1: string, ns2: string) => string>;
    static readonly JSII_3004_INVALID_SUPERTYPE: Code<(clause: ts.HeritageClause, badDeclaration: ts.Declaration) => string>;
    static readonly JSII_3005_TYPE_USED_AS_INTERFACE: Code<(badType: spec.TypeReference) => string>;
    static readonly JSII_3006_TYPE_USED_AS_CLASS: Code<(badType: spec.TypeReference) => string>;
    static readonly JSII_3007_ILLEGAL_STRUCT_EXTENSION: Code<(offender: spec.Type, struct: spec.InterfaceType) => string>;
    static readonly JSII_3008_STRUCT_PROPS_MUST_BE_READONLY: Code<(propName: string, struct: spec.InterfaceType) => string>;
    static readonly JSII_3009_OPTIONAL_PARAMETER_BEFORE_REQUIRED: Code<(param: spec.Parameter, nextParam: spec.Parameter) => string>;
    static readonly JSII_3999_INCOHERENT_TYPE_MODEL: Code<(messageText: any) => any>;
    static readonly JSII_4000_FAILED_TSCONFIG_VALIDATION: Code<(config: string, ruleSet: string, violations: Array<Violation>) => string>;
    static readonly JSII_4009_DISABLED_TSCONFIG_VALIDATION: Code<(config: string) => string>;
    static readonly JSII_5000_JAVA_GETTERS: Code<(badName: string, typeName: string) => string>;
    static readonly JSII_5001_JAVA_SETTERS: Code<(badName: string, typeName: string) => string>;
    static readonly JSII_5002_OVERRIDE_CHANGES_VISIBILITY: Code<(newElement: string, action: string, newValue: "protected" | "public", oldValue: "protected" | "public") => string>;
    static readonly JSII_5003_OVERRIDE_CHANGES_RETURN_TYPE: Code<(newElement: string, action: string, newValue: string, oldValue: string) => string>;
    static readonly JSII_5004_OVERRIDE_CHANGES_PROP_TYPE: Code<(newElement: string, action: string, newType: spec.TypeReference, oldType: spec.TypeReference) => string>;
    static readonly JSII_5005_OVERRIDE_CHANGES_PARAM_COUNT: Code<(newElement: string, action: string, newCount: number, oldCount: number) => string>;
    static readonly JSII_5006_OVERRIDE_CHANGES_PARAM_TYPE: Code<(newElement: string, action: string, newParam: spec.Parameter, oldParam: spec.Parameter) => string>;
    static readonly JSII_5007_OVERRIDE_CHANGES_VARIADIC: Code<(newElement: string, action: string, newVariadic?: any, oldVariadic?: any) => string>;
    static readonly JSII_5008_OVERRIDE_CHANGES_PARAM_OPTIONAL: Code<(newElement: string, action: string, newParam: spec.Parameter, oldParam: spec.Parameter) => string>;
    static readonly JSII_5009_OVERRIDE_CHANGES_PROP_OPTIONAL: Code<(newElement: string, action: string, newOptional?: any, oldOptional?: any) => string>;
    static readonly JSII_5010_OVERRIDE_CHANGES_MUTABILITY: Code<(newElement: string, action: string, newReadonly?: any, oldReadonly?: any) => string>;
    static readonly JSII_5011_SUBMODULE_NAME_CONFLICT: Code<(submoduleName: string, typeName: string, reserved: readonly string[]) => string>;
    static readonly JSII_5012_NAMESPACE_IN_TYPE: Code<(typeName: string, namespaceName: string) => string>;
    static readonly JSII_5013_STATIC_INSTANCE_CONFLICT: Code<(member: string, type: spec.ClassType) => string>;
    static readonly JSII_5014_INHERITED_STATIC_CONFLICT: Code<(member: spec.Method | spec.Property, type: spec.ClassType, baseMember: spec.Method | spec.Property, baseType: spec.ClassType) => string>;
    static readonly JSII_5015_REDECLARED_INTERFACE_MEMBER: Code<(memberName: string, iface: spec.InterfaceType) => string>;
    static readonly JSII_5016_PROHIBITED_MEMBER_NAME: Code<(badName: string) => string>;
    static readonly JSII_5017_POSITIONAL_KEYWORD_CONFLICT: Code<(badName: string) => string>;
    static readonly JSII_5018_RESERVED_WORD: Code<(badName: string, languages: readonly string[]) => string>;
    static readonly JSII_5019_MEMBER_TYPE_NAME_CONFLICT: Code<(memberKind: "method" | "property", memberSymbol: ts.Symbol, declaringType: spec.Type) => string>;
    static readonly JSII_5020_STATIC_MEMBER_CONFLICTS_WITH_NESTED_TYPE: Code<(nestingType: spec.Type, staticMember: spec.Property | spec.Method | spec.EnumMember, nestedType: spec.Type) => string>;
    static readonly JSII_5021_ABSTRACT_CLASS_MISSING_PROP_IMPL: Code<(intf: spec.InterfaceType, cls: spec.ClassType, prop: string) => string>;
    static readonly JSII_7000_NON_EXISTENT_PARAMETER: Code<(method: spec.Method, param: string) => string>;
    static readonly JSII_7001_ILLEGAL_HINT: Code<(hint: keyof TypeSystemHints, ...valid: readonly string[]) => string>;
    static readonly JSII_7999_DOCUMENTATION_ERROR: Code<(messageText: any) => any>;
    static readonly JSII_8000_PASCAL_CASED_TYPE_NAMES: Code<(badName: string, expectedName?: string) => string>;
    static readonly JSII_8001_ALL_CAPS_ENUM_MEMBERS: Code<(badName: string, typeName: string) => string>;
    static readonly JSII_8002_CAMEL_CASED_MEMBERS: Code<(badName: string, typeName: string) => string>;
    static readonly JSII_8003_STATIC_CONST_CASING: Code<(badName: string, typeName: string) => string>;
    static readonly JSII_8004_SUBMOULE_NAME_CASING: Code<(badName: string) => string>;
    static readonly JSII_8005_INTERNAL_UNDERSCORE: Code<(badName: string) => string>;
    static readonly JSII_8006_UNDERSCORE_INTERNAL: Code<(badName: string) => string>;
    static readonly JSII_8007_BEHAVIORAL_INTERFACE_NAME: Code<(badName: string) => string>;
    static readonly JSII_9000_UNKNOWN_MODULE: Code<(moduleName: any) => string>;
    static readonly JSII_9001_TYPE_NOT_FOUND: Code<(typeRef: spec.NamedTypeReference) => string>;
    static readonly JSII_9002_UNRESOLVEABLE_TYPE: Code<(reference: string) => string>;
    static readonly JSII_9003_UNRESOLVEABLE_MODULE: Code<(location: string) => string>;
    static readonly JSII_9004_UNABLE_TO_COMPUTE_SIGNATURE: Code<(methodName: string, type: spec.Type) => string>;
    static readonly JSII_9996_UNNECESSARY_TOKEN: Code<() => string>;
    static readonly JSII_9997_UNKNOWN_ERROR: Code<(error: Error) => string>;
    static readonly JSII_9998_UNSUPPORTED_NODE: Code<(kindOrMessage: ts.SyntaxKind | string) => string>;
    /**
     * Determines whether a `Diagnostic` instance is a `JsiiDiagnostic` or not.
     * @param diag
     */
    static isJsiiDiagnostic(diag: ts.Diagnostic): diag is JsiiDiagnostic;
    private static readonly JSII_9999_RELATED_INFO;
    /**
     * This symbol unequivocally identifies the `JsiiDiagnostic` domain.
     */
    private static readonly DOMAIN;
    private readonly domain;
    readonly category: ts.DiagnosticCategory;
    readonly code: number;
    readonly jsiiCode: number;
    readonly messageText: string | ts.DiagnosticMessageChain;
    readonly file: ts.SourceFile | undefined;
    readonly start: number | undefined;
    readonly length: number | undefined;
    readonly relatedInformation: ts.DiagnosticRelatedInformation[];
    addRelatedInformation(node: ts.Node, message: JsiiDiagnostic['messageText']): this;
    /**
     * Links the provided `node` with the specified `message` as related to the
     * current diagnostic, unless `node` is undefined.
     *
     * @param node the node where the message should be attached, if any.
     * @param message the message to be attached to the diagnostic entry.
     *
     * @returns `this`
     */
    addRelatedInformationIf(node: ts.Node | undefined, message: JsiiDiagnostic['messageText']): this;
    /**
     * Adds related information to this `JsiiDiagnostic` instance if the provided
     * `node` is defined.
     *
     * @param node    the node to bind as related information, or `undefined`.
     * @param message the message to attach to the related information.
     *
     * @returns `this`
     */
    maybeAddRelatedInformation(node: ts.Node | undefined, message: JsiiDiagnostic['messageText']): this;
    /**
     * Formats this diagnostic with color and context if possible, and returns it.
     * The formatted diagnostic is cached, so that it can be re-used. This is
     * useful for diagnostic messages involving trivia -- as the trivia may have
     * been obliterated from the `SourceFile` by the `TsCommentReplacer`, which
     * makes the error messages really confusing.
     */
    format(projectRoot: string): string;
}
export type DiagnosticMessageFormatter = (...args: any[]) => JsiiDiagnostic['messageText'];
export declare function configureCategories(records: {
    [code: string]: ts.DiagnosticCategory;
}): void;
//# sourceMappingURL=jsii-diagnostic.d.ts.map