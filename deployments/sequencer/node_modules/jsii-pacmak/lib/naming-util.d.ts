/**
 * Converts a jsii method/property names to pascal-case.
 *
 * This is different from `toPascalCase()` since it only capitalizes the first
 * letter. This handles avoids duplicates of things like `toIsoString()` and `toISOString()`.
 * The assumption is that the jsii name is camelCased.
 *
 * @param camelCase The original jsii method name
 * @returns A pascal-cased method name.
 */
export declare function jsiiToPascalCase(camelCase: string): string;
//# sourceMappingURL=naming-util.d.ts.map