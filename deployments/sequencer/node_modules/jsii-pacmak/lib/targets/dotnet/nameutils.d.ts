import * as spec from '@jsii/spec';
export declare class DotNetNameUtils {
    convertPropertyName(original: string): string;
    convertTypeName(original: string): string;
    convertMethodName(original: string): string;
    convertEnumMemberName(original: string): string;
    convertInterfaceName(original: spec.InterfaceType): string;
    convertClassName(original: spec.ClassType | spec.InterfaceType): string;
    convertPackageName(original: string): string;
    convertParameterName(original: string): string;
    capitalizeWord(original: string): string;
    private isInvalidName;
    private escapeParameterName;
    private slugify;
}
//# sourceMappingURL=nameutils.d.ts.map