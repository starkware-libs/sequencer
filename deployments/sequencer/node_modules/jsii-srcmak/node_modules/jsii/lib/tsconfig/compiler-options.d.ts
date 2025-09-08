import * as ts from 'typescript';
export declare const BASE_COMPILER_OPTIONS: ts.CompilerOptions;
/**
 * Helper function to convert a TS enum into a list of allowed values,
 * converting everything to camel case.
 * This is used for example for the watch options
 */
export declare function enumAsCamel(enumData: Record<string, string | number>): string[];
/**
 * Helper function to convert a TS enum into a list of allowed values,
 * converting everything to lower case.
 * This is used for example for the "target" compiler option
 */
export declare function enumAsLower(enumData: Record<string, string | number>): string[];
/**
 * Helper function to convert a TS enum into a list of allowed values,
 * converting everything to kebab case.
 * This is used for example for the "jsx" compiler option
 */
export declare function enumAsKebab(enumData: Record<string, string | number>): string[];
/**
 * The compilerOptions in the programmatic API are slightly differently than the format used in tsconfig.json
 * This helper performs the necessary conversion from the programmatic API format the one used in tsconfig.json
 *
 * @param opt compilerOptions in programmatic API format
 * @returns compilerOptions ready to be written on disk
 */
export declare function convertForJson(opt: ts.CompilerOptions): ts.CompilerOptions;
/**
 * Convert an internal enum value to what a user would write in tsconfig.json
 * Possibly using a converter function to adjust casing.
 * @param value The internal enum value
 * @param enumObj The enum object to convert from
 * @param converter The converter function, defaults to lowercase
 * @returns The humanized version of the enum value
 */
export declare function convertEnumToJson<T>(value: keyof T, enumObj: T, converter?: (value: string) => string): string;
/**
 * Convert the internal lib strings to what a user would write in tsconfig.json
 * @param input The input libs array
 * @returns The humanized version lib array
 */
export declare function convertLibForJson(input: string[]): string[];
/**
 * This is annoying - the values expected in the tsconfig.json file are not
 * the same as the enum constant names, or their values. So we need this
 * function to map the "compiler API version" to the "tsconfig.json version"
 *
 * @param newLine the compiler form of the new line configuration
 *
 * @return the equivalent value to put in tsconfig.json
 */
export declare function convertNewLineForJson(newLine: ts.NewLineKind): string;
//# sourceMappingURL=compiler-options.d.ts.map